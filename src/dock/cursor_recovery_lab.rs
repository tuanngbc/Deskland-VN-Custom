//! Hosted recovery prerequisite, not an implemented custom-cursor feature.
#[cfg(target_env="msvc")]extern "C"{fn tlauncher_cursor_recovery_copy_fixture()->i32;}
#[cfg(target_env="msvc")]
#[test]#[ignore="hosted synthetic ANI: refuse lossy copies, verify independent file reload; no global cursor or registry writes"]
fn native_cursor_recovery_rejects_lossy_animation_and_verifies_file_reload(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    let status=unsafe{tlauncher_cursor_recovery_copy_fixture()};
    assert!(matches!(status,0|100),"native recovery prerequisite failed at stage {status}");
    println!("FLUIDWIN_CURSOR_RECOVERY_LAB synthetic_file_reload_two_frames=VERIFIED copyimage_animation={} hotspot=VERIFIED private_source_destroy=VERIFIED private_copies=RELEASED global_cursor_change=NONE registry_write=NONE guardian_integration=NOT_IMPLEMENTED custom_cursor_feature=NOT_STARTED",if status==0{"EXACT_ON_THIS_RUNNER"}else{"LOSSY_BACKUP_REFUSED"});
}
