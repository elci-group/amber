Output out/amber-propose.gif
Set Shell bash
Set FontSize 14
Set Width 1180
Set Height 640
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
Type "amber --propose --threshold 60 ."
Enter
Sleep 6s
