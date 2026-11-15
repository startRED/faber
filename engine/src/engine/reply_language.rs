//! The reply's language, named on every turn (the owner, 2026-11-14: a long project turn full of English code, tool
//! output and helper reports drifted into English after a Portuguese request). The persona states the rule once, in the
//! abstract; each turn that goes out (the person's, and one Faber sends by itself) now ends with a short note naming the
//! language of the person's latest own words. English needs no note: the drift goes towards it. No state, no process:
//! one scan of the conversation's last person message per turn.

use super::*;

/// Letters and small words only one of the screen's other languages uses (as `ui/language.ts`), with the words a short
/// request is made of ("vc consegue arrumar?"), so a request of three words still names its language.
const MARKS: [(&str, &str, &str); 4] = [
    (
        "pt-BR",
        "ãõ",
        "de da das dos para pra com em na nos nas e ou o os um uma uns umas por pelo pela sem ao aos que não sobre entre até mais seu sua cada também tbm você vc isso esse essa está tá é como deveria eu meu minha quero preciso consegue pode faz fazer arruma arrumar corrige corrija conserta deu erro agora aqui ainda tudo muito obrigado",
    ),
    (
        "es",
        "ñ¿¡",
        "de del la el los las para y o un una por al que sobre entre hasta más su sus cada también usted está como yo mi quiero necesito puedes puede hacer arregla arreglar corrige esto aquí ahora todavía muy gracias",
    ),
    (
        "fr",
        "èëîïœùûÿ",
        "de du des la les pour avec et ou un une par sans au aux sur dans que chaque aussi vous est comme ne pas je mon ma veux besoin peux peut faire corrige corriger ceci ici maintenant encore tout très merci",
    ),
    (
        "de",
        "äöüß",
        "der das den dem des und oder für mit ein eine einen zu zum zur von vom auf aus bei nach über im ist nicht auch sie wie ich mein meine will brauche kannst kann machen behebe beheben dies hier jetzt noch alles sehr danke bitte",
    ),
];
// Language codes ("es", "en") and words English text carries alone ("le parser") are left out of the lists above.
const ENGLISH: &str = "the and to of is you that it with for this on be are was have not what can please in at from by an into";

/// The screen language a message reads in, when it is not English: the language with the most marks (a word, or one of
/// its letters once), a unique best, and more than half the English words, since a request may quote some. Two marks
/// decide; one decides for the language the person chose for Faber's screen (`screen`), so "Pode continuar?" on a
/// Portuguese screen is Portuguese. Fenced and inline code, quoted lines and names are no one's words: a message of code,
/// a log, a link or a name ("Fix João") has none.
// shortcut: a long English log pasted without a fence or `>` can outweigh a short request; the persona's own rule still holds.
pub(super) fn language_of(text: &str, screen: Option<&str>) -> Option<&'static str> {
    let unquoted: String = text.lines().filter(|line| !line.trim_start().starts_with('>')).collect::<Vec<_>>().join(
        "
",
    );
    let own: String = unquoted.split("```").step_by(2).flat_map(|part| part.split('`').step_by(2)).collect::<Vec<_>>().join(" ");
    // A link or a path ("https://example.com/pt-br/como", "src/de/main.rs") is no one's words either.
    let own: String = own.split_whitespace().filter(|token| !token.contains(['/', '\\'])).collect::<Vec<_>>().join(" ");
    let tokens: Vec<&str> = own.split(|c: char| !c.is_alphabetic()).filter(|w| !w.is_empty()).collect();
    let capital = |at: usize| tokens.get(at).is_some_and(|w| w.starts_with(char::is_uppercase));
    // A name is no one's language: a capitalised word past the first, and a particle between two ("João da Silva").
    let lower = tokens
        .iter()
        .enumerate()
        .filter(|&(at, w)| !(at > 0 && capital(at)) && !(at > 0 && capital(at - 1) && capital(at + 1) && w.len() <= 4))
        .map(|(_, w)| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<&str> = lower.split(' ').filter(|w| !w.is_empty()).collect();
    // Each word counts once: a request that repeats one shared word ("the todo list") is not a language.
    let distinct: std::collections::BTreeSet<&str> = words.iter().copied().collect();
    let count = |list: &str| distinct.iter().filter(|w| list.split(' ').any(|m| m == **w)).count();
    let scores = MARKS.map(|(code, letters, small)| (code, count(small) + usize::from(lower.chars().any(|c| letters.contains(c)))));
    let english = count(ENGLISH);
    let best = scores.iter().max_by_key(|(_, score)| *score)?;
    let unique = scores.iter().filter(|(_, score)| *score == best.1).count() == 1;
    let enough = best.1 >= 2 || (best.1 == 1 && english == 0 && screen == Some(best.0));
    (enough && best.1 * 2 > english && unique).then_some(best.0)
}

/// Faber's note after a turn's text, naming the reply's language; framed as Faber's, like the clarifying note.
pub(super) fn note(code: &str) -> Option<String> {
    let name = crate::persona::language_name(code)?;
    Some(format!(
        "(From Faber, not the person: the person writes in {name}. Write every message they read in {name}, also late in long work, whatever language the code, files, tools or helpers use.)"
    ))
}

impl Core {
    /// The turn's text with the language note, for a conversation whose person last wrote in another language than
    /// English. A slash command goes out as it is: the CLI reads its first word.
    pub(super) fn language_kept(&self, id: &str, text: String) -> String {
        let last = self.convs.get(id).and_then(|c| c.messages.iter().rev().find(|m| m.role == Role::User && m.wake.is_none()));
        let words = last.map(|m| m.text.clone()).unwrap_or_default();
        with_note(text, &words, Some(self.screen_code()))
    }

    /// The code of the language Faber's screen shows (the choice in Settings, else the system's).
    pub(super) fn screen_code(&self) -> &'static str {
        ["en", "pt-BR", "es", "fr", "de"][self.screen_language() as usize]
    }
}

/// `text` with the note naming the language `words` read in, if any. A message handed to a running turn is decided by
/// its own words, which are not in the conversation yet.
pub(super) fn with_note(text: String, words: &str, screen: Option<&str>) -> String {
    if text.starts_with('/') {
        return text;
    }
    match language_of(words, screen).and_then(note) {
        Some(note) => format!(
            "{text}

{note}"
        ),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a screen that follows the system: only two marks decide.
    fn language_of_en(text: &str) -> Option<&'static str> {
        language_of(text, None)
    }

    #[test]
    fn a_message_names_its_language_and_english_code_or_logs_name_none() {
        assert_eq!(language_of_en("E ai como tá indo? e tbm corrija para o faber não responder na linguagem errada mais"), Some("pt-BR"));
        assert_eq!(language_of_en("Faça com que o faber de mais liberdade e permissão aos agentes"), Some("pt-BR"));
        assert_eq!(language_of_en("¿Puedes revisar la página de inicio y corregir el menú?"), Some("es"));
        assert_eq!(language_of_en("Peux-tu corriger la page d'accueil et vérifier le menu sur mobile ?"), Some("fr"));
        assert_eq!(language_of_en("Kannst du die Startseite prüfen und das Menü für das Handy ändern?"), Some("de"));
        assert_eq!(language_of_en("Please fix the login page and check that the menu works on a phone"), None);
        assert_eq!(language_of_en("error[E0425]: cannot find value `x` in this scope --> src/main.rs:4:5"), None);
        assert_eq!(language_of_en("https://example.com/a/b?c=d"), None);
        assert_eq!(language_of_en(""), None);
        assert_eq!(language_of_en("vc consegue arrumar?"), Some("pt-BR"), "a short request");
        assert_eq!(language_of_en("Fix João"), None, "a name is not a language");
        assert_eq!(language_of_en("Please add a todo and fix the todo list"), None, "an English word in a marker list");
        assert_eq!(language_of_en("Fix the page at https://example.com/pt-br/como/pode"), None, "a link is not words");
        assert_eq!(language_of_en("Please fix the sin and con functions"), None, "function names are not Spanish");
        assert_eq!(language_of_en("Open src/de/la/main.rs and fix it"), None);
        assert_eq!(language_of("Change es to en in config", Some("es")), None, "language codes are not Spanish");
        assert_eq!(language_of("Fix le parser error", Some("fr")), None);
        assert_eq!(language_of("Corrige le menu, merci", Some("fr")), Some("fr"), "a short French request");
        assert_eq!(language_of("Corrige le menu", Some("fr")), None, "\"corrige\" is Portuguese and Spanish too: no guess");
        assert_eq!(language_of_en("Fix the menu for João da Silva"), None, "nor a name with its particle");
        assert_eq!(language_of_en("Corrija o menu do João da Silva, por favor"), Some("pt-BR"));
        assert_eq!(language_of_en("Bitte Fenster öffnen"), Some("de"), "a German request keeps its marks past its nouns");
        assert_eq!(
            language_of_en(
                "Fix the header, he wrote:
> Corrija o menu da tela de login para mim"
            ),
            None,
            "a quotation is not theirs"
        );
        for short in ["Pode continuar?", "Arruma a tela"] {
            assert_eq!(language_of(short, Some("pt-BR")), Some("pt-BR"), "{short} on a Portuguese screen");
        }
        assert_eq!(language_of("Pode continuar?", Some("en")), None, "one mark alone decides only for the chosen screen");
        assert_eq!(language_of("Fix the o-ring", Some("pt-BR")), None);
        assert_eq!(language_of_en("Please fix the bug on the página de login"), None);
        assert_eq!(
            language_of_en(
                "Deu esse erro, consegue ver?
```
Error: the module is not found and the build is not done
```"
            ),
            Some("pt-BR")
        );
        assert_eq!(language_of_en("Deu esse erro, consegue ver? Error: Cannot find module react, the stack is not there"), Some("pt-BR"));
    }

    #[test]
    fn a_slash_command_goes_out_untouched_and_english_gets_no_note() {
        assert_eq!(with_note("/compact".into(), "Corrija o erro de login", None), "/compact");
        assert_eq!(with_note("Fix it".into(), "Fix the login page", None), "Fix it");
        assert!(
            with_note("Corrija".into(), "Corrija o erro de login", None)
                .ends_with("in Brazilian Portuguese, also late in long work, whatever language the code, files, tools or helpers use.)")
        );
    }

    #[test]
    fn the_note_names_the_language_and_stays_faber_s_own() {
        let text = note("pt-BR").unwrap();
        assert!(text.starts_with("(From Faber, not the person:") && text.ends_with(')'), "{text}");
        assert!(text.contains("Brazilian Portuguese"), "{text}");
        assert_eq!(note("xx"), None);
    }
}
