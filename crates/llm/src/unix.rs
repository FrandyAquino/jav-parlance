//! A lifetime pipe also closes when the owning app crashes or is SIGKILLed.
//! The guardian keeps the server out of the foreground process and kills it
//! on EOF; no model or path is interpolated into shell source.

use std::path::Path;

use tokio::process::Command;

const GUARDIAN: &str = r#"
exec 3<&0
"$@" 3<&- </dev/null &
server=$!
(
    IFS= read -r ignored
    kill -KILL "$server" 2>/dev/null
) <&3 &
watcher=$!
exec 3<&-
wait "$server"
result=$?
kill -KILL "$watcher" 2>/dev/null
wait "$watcher" 2>/dev/null
exit "$result"
"#;

pub(crate) fn command(executable: &Path) -> std::io::Result<Command> {
    let executable = executable.canonicalize()?;
    let mut command = Command::new("/bin/sh");
    command.args(["-c", GUARDIAN, "parlance-guardian"]).arg(executable);
    Ok(command)
}
