"""Build tn-tsv9/en_tn_57-TIT.tsv from the real en_tn v86 Titus TSV7 file.

Usage: python3 -I make_tsv9.py <en_tn@v86/TIT.tsv> <output.tsv>

The legacy nine-column layout is
Book, Chapter, Verse, ID, SupportReference, OrigQuote, Occurrence, GLQuote, OccurrenceNote.
This script takes the first 12 rows of the TSV7 file and lays them out that way:
`front:intro` becomes Chapter `front`, Verse `intro`; newlines (`\\n` in TSV7) become
`<br>`; GLQuote is filled for rows listed in GL_QUOTES (the ULT rendering) and left
empty otherwise; every third SupportReference is shortened to the bare slug, the older
form. The rows are real; only the layout is constructed.
"""

import sys

GL_QUOTES = {
    'rtc9': 'for the faith of the chosen people of God and knowledge of the truth',
    'xrtm': 'the chosen people of God',
}

HEADER = 'Book\tChapter\tVerse\tID\tSupportReference\tOrigQuote\tOccurrence\tGLQuote\tOccurrenceNote'


def main(src, dst):
    lines = open(src, encoding='utf-8').read().split('\n')
    assert lines[0] == 'Reference\tID\tTags\tSupportReference\tQuote\tOccurrence\tNote'
    out = [HEADER]
    for n, line in enumerate(l for l in lines[1:13] if l):
        ref, rid, _tags, support, quote, occ, note = line.split('\t')
        chapter, verse = ref.split(':')
        if n % 3 == 2 and support:
            support = support.rsplit('/', 1)[1]
        note = note.replace('\\n', '<br>')
        out.append('\t'.join(['TIT', chapter, verse, rid, support, quote, occ, GL_QUOTES.get(rid, ''), note]))
    with open(dst, 'w', encoding='utf-8') as f:
        f.write('\n'.join(out) + '\n')


if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
