; JuiceBox Trainer installer hooks (v1.23.0).
;
; OpenTrainer was renamed JuiceBox Trainer. Before installing, remove the old
; OpenTrainer program (its files, shortcuts and its entry in Installed apps) so
; there aren't two. Its uninstaller deletes only its own program files: the
; player's packs, saves and settings stay where they are, and JuiceBox Trainer
; moves them to its own folders on its first start.
!macro NSIS_HOOK_PREINSTALL
  IfFileExists "$LOCALAPPDATA\OpenTrainer\uninstall.exe" 0 jb_no_opentrainer
    DetailPrint "Removing OpenTrainer (renamed JuiceBox Trainer)..."
    ExecWait '"$LOCALAPPDATA\OpenTrainer\uninstall.exe" /S _?=$LOCALAPPDATA\OpenTrainer'
    ; With _?= the uninstaller can't remove itself.
    Delete "$LOCALAPPDATA\OpenTrainer\uninstall.exe"
    ; Only if nothing is left in it (the player's things are moved on first start).
    RMDir "$LOCALAPPDATA\OpenTrainer"
  jb_no_opentrainer:
!macroend
