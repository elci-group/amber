Output out/amber-emoji-analyze.gif
Set Shell bash
Set FontSize 14
Set Width 1000
Set Height 560
Set Padding 24
Set Framerate 30
Set TypingSpeed 50ms
Set Theme "Catppuccin Mocha"

Type "export PS1='> '"
Enter
Type "export RUST_LOG=error"
Enter
Type "clear"
Enter
Sleep 300ms

Type "cd fixture"
Enter
Type "amber --format emoji --threshold 0 ."
Enter
Sleep 3.5s
