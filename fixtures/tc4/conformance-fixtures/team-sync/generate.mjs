// Generates the shared team-sync fixture (D98, issue #674, epic #24): the journals of three
// translators, A, B and C, on ONE project, plus two broken files kept apart in `cases/`.
// README.md in this directory names each case; cases.json is the same index as data.
//
// Every journal byte comes from the reference writers: A's creation is the §8.8 seed of the
// conformance sample (`seedFromSidecars`), and each later action is sealed and written by
// `writeActionSegment` (journal/files.mjs). B and C base their edits on A's creation, as two
// computers do after "Open a project from Door43"; they never seed a second time (D53). Only
// `actor.json` (the app's own provisioning form, journalStore.ts) and the two deliberately
// broken files are written here by hand. Every ts comes from a fixed clock, so a second run
// writes the same bytes; JC-35 checks that.
//
//   node conformance/fixtures/team-sync/generate.mjs        (also run by `npm run generate`)
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { makeClock } from '../../../journal/hlc.mjs';
import { seedFromSidecars } from '../../../journal/reconcile.mjs';
import { fold } from '../../../journal/fold.mjs';
import { identityKeyOf } from '../../../journal/grammar.mjs';
import { sealAction, writeActionSegment, actorDirFor, segmentName } from '../../../journal/files.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SAMPLE = path.resolve(HERE, '../../sample-burrito');

export const ACTORS = { a: 'translator-a', b: 'translator-b', c: 'translator-c' };
// Fixture usernames: they match the Door43 grammar (R-8.5.25); they name no real account.
export const USERNAMES = { 'translator-a': 'fixture-translator-a', 'translator-b': 'fixture-translator-b', 'translator-c': 'fixture-translator-c' };

/** The sample's inputs, read the way JC-34e and buildSeed read them. */
export const sampleInputs = (sampleDir = SAMPLE) => {
  const ING = (p) => path.join(sampleDir, 'ingredients', p);
  const read = (p) => fs.readFileSync(ING(p), 'utf8');
  return {
    books: { TIT: read('TIT.usfm'), JON: read('JON.usfm') },
    decisionFiles: {
      translationWords: JSON.parse(read('checking/translationWords/TIT.json')),
      translationNotes: JSON.parse(read('checking/translationNotes/TIT.json')),
    },
    alignmentFiles: { TIT: JSON.parse(read('checking/alignments/TIT.json')) },
    vrs: { name: 'eng', bytes: read('vrs.json') },
    baseMetadata: JSON.parse(fs.readFileSync(path.join(sampleDir, 'metadata.json'), 'utf8')),
  };
};

/** Build the fixture into `outDir` (journals/, cases/, cases.json). Returns the case index. */
export const buildTeamSyncFixture = (outDir = HERE, sampleDir = SAMPLE) => {
  for (const p of ['journals', 'cases', 'cases.json']) fs.rmSync(path.join(outDir, p), { recursive: true, force: true });
  const journalDir = path.join(outDir, 'journals');
  const rel = (file) => path.relative(outDir, file).split(path.sep).join('/');
  const s = sampleInputs(sampleDir);

  // ---- A creates the project: the §8.8 seed of the sample, one action ----
  const seed = seedFromSidecars({ actor: ACTORS.a, books: s.books, decisionFiles: s.decisionFiles, alignmentFiles: s.alignmentFiles, vrs: s.vrs });
  const seeded = fold(seed);
  const titRoot = seed.find((e) => e.op === 'book.add' && e.book === 'TIT').ts;

  // one fixed clock per actor; `at` sets the physical time of the next action
  let now = 0;
  const clocks = Object.fromEntries(Object.values(ACTORS).map((a) => [a, makeClock(a, () => now)]));
  for (const c of Object.values(clocks)) c.ratchet(seed[seed.length - 1].ts);
  const issue = (actor, iso) => { now = Date.parse(iso); return clocks[actor].issue(); };

  const actorDir = (actor) => actorDirFor(journalDir, actor);
  const write = (actor, events) => rel(writeActionSegment(actorDir(actor), events));
  const actorJson = (actor) => {
    fs.mkdirSync(actorDir(actor), { recursive: true });
    // the app's provisioning form (src/data/journal/journalStore.ts open())
    const doc = { schemaVersion: 1, actorId: actor, createdAt: '2026-10-13T08:00:00.000Z', device: 'translation device' };
    fs.writeFileSync(path.join(actorDir(actor), 'actor.json'), JSON.stringify(doc, null, 2));
    return rel(path.join(actorDir(actor), 'actor.json'));
  };
  const verseEdit = (actor, iso, verse, text) => ({
    v: 1, op: 'text.verse.set', actor, ts: issue(actor, iso),
    base: seeded.headsTs[`text|TIT|${verse}`], book: 'TIT', chapter: verse.split(':')[0], verse: verse.split(':')[1], text,
  });

  const files = {};
  for (const actor of Object.values(ACTORS)) files[actor] = { actor: actorJson(actor), segments: [] };
  const seedFile = write(ACTORS.a, seed);
  files[ACTORS.a].segments.push(seedFile);

  // ---- names: each translator records its own Door43 username, once (R-8.5.25) ----
  const nameFiles = {};
  for (const [i, actor] of Object.values(ACTORS).entries()) {
    const ev = { v: 3, op: 'actor.name.set', actor, ts: issue(actor, `2026-10-13T09:00:0${i}.000Z`), base: null, username: USERNAMES[actor] };
    nameFiles[actor] = write(actor, [ev]);
    files[actor].segments.push(nameFiles[actor]);
  }

  // ---- different-verses: A, B and C each change a different verse ----
  const a11 = verseEdit(ACTORS.a, '2026-10-13T10:00:00.000Z', '1:1', 'Pablo, siervo de Dios y apóstol de Jesucristo, para la fe de los escogidos de Dios y el conocimiento de la verdad que lleva a la piedad,\n');
  const b12 = verseEdit(ACTORS.b, '2026-10-13T10:01:00.000Z', '1:2', 'en la esperanza de la vida eterna, que Dios, que no miente, prometió antes del principio de los siglos,\n');
  const c13 = verseEdit(ACTORS.c, '2026-10-13T10:02:00.000Z', '1:3', 'y a su tiempo manifestó su palabra por la predicación que me fue confiada por mandato de Dios nuestro Salvador,\n');
  const differentVerses = [write(ACTORS.a, [a11]), write(ACTORS.b, [b12]), write(ACTORS.c, [c13])];
  files[ACTORS.a].segments.push(differentVerses[0]); files[ACTORS.b].segments.push(differentVerses[1]); files[ACTORS.c].segments.push(differentVerses[2]);

  // ---- verse-two-ways: A and B change TIT 1:4 from the same base — a verse-text fork ----
  const a14 = verseEdit(ACTORS.a, '2026-10-13T10:10:00.000Z', '1:4', 'a Tito, verdadero hijo en nuestra fe común: Gracia y paz de Dios Padre y de Cristo Jesús nuestro Salvador.\n');
  const b14 = verseEdit(ACTORS.b, '2026-10-13T10:11:00.000Z', '1:4', 'a Tito, mi hijo verdadero en la fe que compartimos: Gracia y paz de parte de Dios Padre y de Cristo Jesús nuestro Salvador.\n');
  const verseTwoWays = [write(ACTORS.a, [a14]), write(ACTORS.b, [b14])];
  files[ACTORS.a].segments.push(verseTwoWays[0]); files[ACTORS.b].segments.push(verseTwoWays[1]);

  // ---- check-two-ways: B and C set one tW check result two ways — a non-text fork ----
  const decision = seeded.decisions.translationWords[0];
  const decKey = `dec|translationWords|${identityKeyOf(decision.contextId)}`; // the fold's own register key
  if (!seeded.headsTs[decKey]) throw new Error('team-sync fixture: the sample decision key was not found in the seed fold');
  const decEvent = (actor, iso, status) => ({
    v: 1, op: 'check.decision.set', actor, ts: issue(actor, iso), base: seeded.headsTs[decKey], generation: titRoot,
    toolId: 'translationWords', decision: { ...decision, status, modifiedTimestamp: iso },
  });
  const bDec = decEvent(ACTORS.b, '2026-10-13T10:20:00.000Z', 'valid');
  const cDec = decEvent(ACTORS.c, '2026-10-13T10:21:00.000Z', 'todo');
  const checkTwoWays = [write(ACTORS.b, [bDec]), write(ACTORS.c, [cDec])];
  files[ACTORS.b].segments.push(checkTwoWays[0]); files[ACTORS.c].segments.push(checkTwoWays[1]);

  // ---- invalid-file (cases/): a C segment whose body no longer matches its sha256 ----
  const c15 = verseEdit(ACTORS.c, '2026-10-13T10:30:00.000Z', '1:5', 'Por esta causa te dejé en Creta, para que pusieras en orden lo que falta y nombraras ancianos en cada ciudad, como te mandé.\n');
  const sealedC15 = JSON.parse(sealAction([c15]));
  const broken = JSON.stringify({ ...sealedC15, body: sealedC15.body.replace('pusieras en orden', 'pusieras orden') });
  const invalidDir = path.join(outDir, 'cases', 'invalid-file', ACTORS.c, 'segments');
  fs.mkdirSync(invalidDir, { recursive: true });
  const invalidPath = path.join(invalidDir, segmentName(c15.ts));
  fs.writeFileSync(invalidPath, broken);

  // ---- changed-own-file (cases/): A's own 1:1 segment path, with a different valid action ----
  const changedDir = path.join(outDir, 'cases', 'changed-own-file', ACTORS.a, 'segments');
  fs.mkdirSync(changedDir, { recursive: true });
  const changedPath = path.join(changedDir, segmentName(a11.ts));
  fs.writeFileSync(changedPath, sealAction([{ ...a11, text: 'Pablo, siervo de Dios y apóstol de Jesucristo, conforme a la fe de los escogidos de Dios,\n' }]));

  const index = {
    fixture: 'team-sync',
    decision: 'D98',
    actors: Object.fromEntries(Object.values(ACTORS).map((a) => [a, { username: USERNAMES[a], ...files[a] }])),
    cases: {
      'creation': { description: 'A creates the project: the seed of the conformance sample (TIT, JON).', segments: [seedFile] },
      'names': { description: 'A, B and C each record their own Door43 username once (actor.name.set, v: 3).', segments: Object.values(nameFiles) },
      'different-verses': { description: 'A changes TIT 1:1, B changes TIT 1:2, C changes TIT 1:3. No fork.', segments: differentVerses, keys: ['text|TIT|1:1', 'text|TIT|1:2', 'text|TIT|1:3'] },
      'verse-two-ways': { description: 'A and B change TIT 1:4 from the same base. A verse-text fork; B is newest.', segments: verseTwoWays, key: 'text|TIT|1:4', newest: b14.ts },
      'check-two-ways': { description: 'B and C set one translationWords check result two ways (status valid, then todo). A non-text fork; C is newest.', segments: checkTwoWays, key: decKey, newest: cDec.ts },
      'invalid-file': { description: 'A segment of C whose body does not match its sha256. Not in journals/.', path: rel(invalidPath), actor: ACTORS.c },
      'changed-own-file': { description: 'A different valid action at the path of A\'s own TIT 1:1 segment. Not in journals/.', path: rel(changedPath), own: differentVerses[0], actor: ACTORS.a },
    },
  };
  fs.writeFileSync(path.join(outDir, 'cases.json'), JSON.stringify(index, null, 2) + '\n');
  return index;
};

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const index = buildTeamSyncFixture();
  console.log(`team-sync fixture: ${Object.keys(index.cases).length} cases, ${Object.values(index.actors).reduce((n, a) => n + a.segments.length, 0)} segments in journals/`);
}
