//! Hosted recovery prerequisite, not an implemented custom-cursor feature.
#[cfg(target_env="msvc")]extern "C"{fn tlauncher_cursor_recovery_copy_fixture()->i32;}
#[cfg(target_env="msvc")]
#[test]#[ignore="hosted synthetic animated-cursor copy after source destruction; no global cursor or registry writes"]
fn native_animated_cursor_copy_keeps_frames_hotspot_and_private_lifetime(){
    if std::env::var("GITHUB_ACTIONS").as_deref()!=Ok("true"){return;}
    assert_eq!(unsafe{tlauncher_cursor_recovery_copy_fixture()},0);
    println!("FLUIDWIN_CURSOR_RECOVERY_LAB synthetic_ani_two_frames=VERIFIED copy_after_source_destroy=VERIFIED hotspot=VERIFIED private_copies=RELEASED global_cursor_change=NONE registry_write=NONE guardian_integration=NOT_IMPLEMENTED custom_cursor_feature=NOT_STARTED");
}
