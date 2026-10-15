"""Cut the Aquifer fixture subset from a checkout of the BibleAquifer repositories.

Usage: python3 -I subset.py <checkout-root> <output-root>

<checkout-root> holds one directory per repository (UWTranslationNotes, ...), each laid
out as in git (<lang>/metadata.json, <lang>/json/*.content.json), at the tag and commit
named in README.md. The script writes the subset described in README.md. Every article
object it copies is re-serialized with json.dumps(indent=0, ensure_ascii=False), which
reproduces the release bytes exactly (checked on whole files before cutting), so a copied
article is byte-identical to its source except where README.md says it was trimmed.
"""

import json
import os
import sys

TRIM_RESOURCES = 20

WORDS = {
    # The five Titus 1:1 terms with the shortest passage lists, in each language.
    'eng': ['26324', '26553', '26583', '26647', '26937'],
    'por': ['237603', '237828', '237858', '237922', '238209'],
}
MANUAL = {
    'eng': ['33257', '33292', '33372', '33322', '33375', '33397', '689645', '33342'],
    'por': ['800036', '800071', '800151', '800101', '800154', '800176', '800224', '800121'],
}
# Whole Titus files for the canonical resources.
CANONICAL = {'UWTranslationNotes': '56', 'UWTranslationQuestions': '56'}
# The article_metadata kept for canonical resources: the book intro, the chapter 1 intro
# and every Titus 1:1 entry.
KEEP_INDEX = {'56000000', '56001000', '56001001'}
# A notes file with articles Aquifer filed under no passage (`index_reference` 00000000):
# (repository, language, book file, how many of its indexed articles to keep beside them).
UNINDEXED = ('UWTranslationNotes', 'fra', '01', 3)


def load(path):
    with open(path, encoding='utf-8') as f:
        return json.load(f)


def dump(obj, path):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w', encoding='utf-8') as f:
        f.write(json.dumps(obj, ensure_ascii=False, indent=0))


def trim(article):
    assoc = article.get('associations') or {}
    res = assoc.get('resource') or []
    if len(res) > TRIM_RESOURCES:
        assoc['resource'] = res[:TRIM_RESOURCES]
    return article


def metadata(src, dst, ingredient_keys, keep_ids, keep_index):
    meta = load(src)
    ing = meta['scripture_burrito']['ingredients']
    meta['scripture_burrito']['ingredients'] = {k: v for k, v in ing.items() if k in ingredient_keys}
    am = meta.get('article_metadata') or {}
    meta['article_metadata'] = {
        k: v for k, v in am.items()
        if k in keep_ids or (keep_index and v.get('index_reference') in keep_index)
    }
    dump(meta, dst)


def main(root, out):
    for repo, book in CANONICAL.items():
        for lang in ('eng', 'por'):
            name = f'json/{book}.content.json'
            articles = load(os.path.join(root, repo, lang, name))
            dump(articles, os.path.join(out, repo, lang, name))
            metadata(os.path.join(root, repo, lang, 'metadata.json'),
                     os.path.join(out, repo, lang, 'metadata.json'), {name}, set(), KEEP_INDEX)
    for repo, wanted, out_name in (('UWTranslationWords', WORDS, 'json/tit-1-1.content.json'),
                                   ('UWTranslationManual', MANUAL, 'json/subset.content.json')):
        for lang in ('eng', 'por'):
            ids = wanted[lang]
            found, sources = {}, set()
            json_dir = os.path.join(root, repo, lang, 'json')
            for name in sorted(os.listdir(json_dir)):
                for article in load(os.path.join(json_dir, name)):
                    if str(article['content_id']) in ids:
                        found[str(article['content_id'])] = trim(article)
                        sources.add('json/' + name)
            missing = [i for i in ids if i not in found]
            if missing:
                sys.exit(f'{repo}/{lang}: missing {missing}')
            dump([found[i] for i in ids], os.path.join(out, repo, lang, out_name))
            metadata(os.path.join(root, repo, lang, 'metadata.json'),
                     os.path.join(out, repo, lang, 'metadata.json'), sources, set(ids), None)
    unindexed(root, out)


def unindexed(root, out):
    """Every article of the file filed under 00000000, and its first few indexed ones, in
    file order."""
    repo, lang, book, keep = UNINDEXED
    articles = load(os.path.join(root, repo, lang, f'json/{book}.content.json'))
    indexed = [a for a in articles if a['index_reference'] != '00000000'][:keep]
    kept = [a for a in articles if a['index_reference'] == '00000000' or a in indexed]
    dump(kept, os.path.join(out, repo, lang, f'json/{book}-subset.content.json'))


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
