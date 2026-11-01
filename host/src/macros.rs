//! The macro that builds the command table.

/// A command's failure type: an error key unless the entry names another (`Failure`, for the appearance commands).
macro_rules! fail {
    () => {
        String
    };
    ($err:ty) => {
        $err
    };
}

/// Builds the commands, `Host::dispatch` and the table from one list, so they cannot drift apart. Each entry is
/// `Kind Reach fn name(host, arg: Type, ...) -> Answer { body }`, where the body returns `Result<Answer, String>`, or
/// `-> Answer [Failure] { body }` for a body that fails with a [`Failure`](crate::Failure). `Reach` says who may send it
/// ([`Reach`](crate::Reach)); `Own("/pointer")` names the conversation it acts on inside the arguments. The doc comment is
/// the command's description in the command line's help and in the AI's tools. A `#[cfg(...)]` before the kind keeps the
/// entry out of the commands, the table and `dispatch` alike.
macro_rules! table {
    ($( $(#[doc = $doc:literal])* $(#[cfg($cfg:meta)])? $kind:ident $reach:ident $(($target:literal))? fn $name:ident($h:ident $(, $arg:ident: $ty:ty)* $(,)?) -> $ret:ty $([$err:ty])? $body:block )*) => {
        mod commands {
            #![allow(clippy::unused_async, unused_variables)]
            use super::*;
            $( $(#[doc = $doc])* $(#[cfg($cfg)])? pub(super) async fn $name($h: &Host $(, $arg: $ty)*) -> Result<$ret, fail!($($err)?)> $body )*
        }

        pub(crate) const TABLE: &[Entry] = &[$( $(#[cfg($cfg)])? Entry {
            name: stringify!($name),
            args: &[$( (stringify!($arg), stringify!($ty)) ),*],
            schema: || argument_schema(&[$( (stringify!($arg), stringify!($ty)) ),*]),
            kind: Kind::$kind,
            reach: Reach::$reach $(($target))?,
            doc: &[$($doc),*],
        } ),*];

        impl Host {
            /// Runs one command. Arguments are an object of named values, as the screen sends them; anything else, or
            /// arguments that do not fit, is `error.host.badArgs`, and a name not in the table `error.host.unknownCommand`.
            pub async fn dispatch(&self, command: &str, args: Value) -> Result<Value, Failure> {
                let args = match args {
                    Value::Null => Value::Object(Default::default()),
                    Value::Object(_) => args,
                    _ => return Err(Failure::key("error.host.badArgs")),
                };
                match command {
                    $( $(#[cfg($cfg)])? stringify!($name) => {
                        #[derive(serde::Deserialize)]
                        #[serde(rename_all = "camelCase")]
                        struct Args { $( $arg: $ty, )* }
                        let Args { $( $arg, )* } = serde_json::from_value(args).map_err(|_| Failure::key("error.host.badArgs"))?;
                        let answer = commands::$name(self $(, $arg)*).await.map_err(Failure::from)?;
                        serde_json::to_value(answer).map_err(|_| Failure::key("error.host.badReply"))
                    } )*
                    _ => Err(Failure::key("error.host.unknownCommand")),
                }
            }
        }
    };
}
