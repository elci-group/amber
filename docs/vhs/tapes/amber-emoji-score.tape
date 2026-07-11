Output out/amber-emoji-score.gif
Set Shell bash
Set FontSize 14
Set Width 1000
Set Height 460
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
Type "amber --format emoji score anyhow"
Enter
Sleep 3s
