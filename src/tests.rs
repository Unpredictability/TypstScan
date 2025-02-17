#[cfg(test)]
mod tests {

    #[test]
    fn test() {
        use std::process::Command;
        let process_name = "scrcpy";
        let window_name = "NoteX3";
        let script = format!(
            r#"
        tell application "System Events"
            set scrcpyWindow to (first window of application process "{process_name}" whose name is "{window_name}")
            perform action "AXRaise" of scrcpyWindow
        end tell
    "#
        );

        let out = Command::new("osascript").arg("-e").arg(script).output().unwrap();
        println!("{:?}", out);
    }

    #[test]
    fn test2() {
        use std::process::Command;
        let process_name = "scrcpyssasd";
        let window_name = "NoteX3";
        let script = format!(
            r#"
        tell application "System Events"
            tell process "{process_name}"
                set frontmost to true
            end tell
        end tell
    "#
        );

        let output = Command::new("osascript").arg("-e").arg(script).output().unwrap();
        println!("{:?}", output);
    }
}
