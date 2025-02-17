#[cfg(test)]
mod tests {

    #[test]
    fn test() {
        assert_eq!(1, 1);
        use std::process::Command;
        let script = r#"
        tell application "System Events"
            set scrcpyWindow to (first window of application process "scrcpy" whose name is "NoteX3")
            perform action "AXRaise" of scrcpyWindow
        end tell
    "#;

        let _ = Command::new("osascript").arg("-e").arg(script).output();
    }
}
