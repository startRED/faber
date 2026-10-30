// Whether a few words read in one of the screen's languages (task 130). A helper's title comes from the CLI (Claude Code's
// `description`, Codex's subagent path) or from the agent (`Helper`'s `title`), and 1,144 of 1,155 the owner saw were
// English on a Portuguese screen. The screen never guesses a translation: a title that does not read in the person's
// language gives way to "Helper 1", and the title stays in the helper's view. The persona evaluation
// (`scripts/persona-eval.mjs`) grades the agent's titles with the same function. No imports, so Node runs it as it is.

/** Words and letters only one of the languages uses (or that its short titles almost always carry). A word English shares
 * (`do`, `no`, `die`) is left out, so "Do security review" never reads as Portuguese; a title with no mark at all
 * ("Testes") gives way to the numbered name, which loses nothing: the helper's view keeps it. */
interface Marks {
  letters: RegExp;
  words: Set<string>;
  /** Endings of a word that English words almost never have. */
  endings?: RegExp;
  /** Verbs a title starts with, beyond the endings. */
  verbs?: Set<string>;
  /** A title's first word in the infinitive (`Revisar`, `Corrigir`); English words with the same ending are in `NOT_VERBS`. */
  infinitive?: RegExp;
}

const words = (list: string) => new Set(list.split(' '));

// English words a title may start with that end like a Portuguese or Spanish infinitive.
const NOT_VERBS = words(
  'similar regular popular sidebar toolbar navbar calendar avatar scalar modular grammar radar solar polar cellular circular singular molecular tabular vulgar sugar cedar pillar dollar collar nectar seminar guitar lunar altar stellar familiar peculiar stir elixir nadir tapir',
);

const MARKS: Record<string, Marks> = {
  'pt-BR': {
    letters: /[ãõçáâàéêíóôú]/,
    words: words('de da das dos para pra com em na nos nas e ou o os um uma uns umas por pelo pela pelos pelas sem ao aos à às que sobre entre até mais seu sua seus suas cada'),
    endings: /(?:ando|endo|indo)$/,
    verbs: words('escrever reescrever resolver fazer entender ler rever remover mover manter obter responder descrever desenvolver escolher recolher conhecer aprender atender estender receber correr esconder devolver envolver proteger'),
    infinitive: /^[a-z]{3,}(?:ar|ir)$/,
  },
  es: {
    letters: /[ñáéíóúü¿¡]/,
    words: words('de del la el los las para con en y o un una unos unas por sin al que sobre entre hasta más su sus cada'),
    endings: /(?:ando|iendo)$/,
    verbs: words('hacer leer resolver entender mover remover mantener obtener responder aprender comprender correr escoger'),
    infinitive: /^[a-z]{3,}(?:ar|ir)$/,
  },
  fr: {
    letters: /[àâæçéèêëîïôœùûüÿ]/,
    words: words('de du des la le les pour avec en et ou un une par sans au aux sur dans que chaque'),
    verbs: words('analyser corriger chercher lire tester examiner comparer explorer trouver regarder lancer mesurer calculer traduire relire ranger nettoyer'),
  },
  de: {
    letters: /[äöüß]/,
    words: words('der das den dem des und oder für mit ein eine einen einem einer zu zum zur von vom auf aus bei nach über im ist nicht jede jeden'),
    endings: /(?:ung|ungen|keit|heit|ieren)$/,
    verbs: words('prüfen suchen lesen schreiben finden testen untersuchen vergleichen zählen berechnen erstellen ändern beheben sammeln aufräumen'),
  },
};

/** A text's words in lower case, with an elided `l'`/`d'` split off as its own word. */
function wordsOf(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/\b([ld])['’]/g, '$1 ')
    .split(/[^\p{L}]+/u)
    .filter(Boolean);
}

/** Whether the words carry a mark of a language. `hits` is how many of its small words it takes: English shares a few
 * (`no`, `do`, `die`), so a title on an English screen reads as another language only with two of them. */
function marked(list: string[], text: string, marks: Marks, hits = 1): boolean {
  if (marks.letters.test(text.toLowerCase())) return true;
  if (list.some((w) => marks.verbs?.has(w) || (w.length > 4 && marks.endings?.test(w)))) return true;
  if (list.filter((w) => marks.words.has(w)).length >= hits) return true;
  const first = list[0] ?? '';
  return !!marks.infinitive?.test(first) && !NOT_VERBS.has(first) && !/(?:ear|air|eir|oir)$/.test(first);
}

/** Whether `text` reads in the screen language `lang` (a code of `ui/i18n.ts`). English is any text with no mark of the
 * other languages; another language needs one of its own marks. Empty text reads in none. */
export function readsIn(text: string, lang: string): boolean {
  const list = wordsOf(text);
  if (!list.length) return false;
  const own = MARKS[lang];
  if (own) return marked(list, text, own);
  return !Object.values(MARKS).some((marks) => marked(list, text, marks, 2));
}
