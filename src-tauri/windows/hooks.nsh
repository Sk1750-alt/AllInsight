; AllInsight installer hooks.
;
; Tauri's uninstaller has a "Delete the application data" checkbox, but what it
; deletes is $LOCALAPPDATA\<bundle identifier> -- info.allinsight.desktop, which
; holds only the WebView cache. AllInsight keeps its own data somewhere else:
; state::data_directory() is %LOCALAPPDATA%\AllInsight. So ticking the box left
; the database, the logs and every imported model on disk.
;
; For a current-user install that folder is also $INSTDIR, which is why the
; uninstaller's own non-recursive RMDir "$INSTDIR" never succeeds: the data is
; still in it.
;
; This removes exactly the items the application creates, by name, and never
; the folder wholesale: the install directory can be chosen at install time, and
; a recursive delete of anything the user picked is not something a cleanup
; tool gets to do by accident.
;
; Keep the names here in step with state.rs (allinsight.db), logging.rs (logs),
; and services/ai/models.rs (models, engine).

!macro NSIS_HOOK_POSTUNINSTALL
  ; Same two conditions as the surrounding Tauri block: only when the user
  ; ticked the box, and never during an update, which would otherwise wipe a
  ; user's imported models on every upgrade.
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current

    Delete "$LOCALAPPDATA\AllInsight\allinsight.db"
    Delete "$LOCALAPPDATA\AllInsight\allinsight.db-wal"
    Delete "$LOCALAPPDATA\AllInsight\allinsight.db-shm"
    RMDir /r "$LOCALAPPDATA\AllInsight\logs"
    RMDir /r "$LOCALAPPDATA\AllInsight\models"
    RMDir /r "$LOCALAPPDATA\AllInsight\engine"

    ; Non-recursive: only succeeds if nothing else is left in the folder.
    RMDir "$LOCALAPPDATA\AllInsight"
  ${EndIf}
!macroend
