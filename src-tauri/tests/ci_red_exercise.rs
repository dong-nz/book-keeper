// AC-157 red exercise — this file exists only on the throwaway branch
// `ci-red-exercise` and must never merge. Purpose: prove that a red test
// layer turns the whole CI pipeline red (NFR-10, SPEC-v0.0.11 AC-157).
#[test]
fn deliberately_red_pipeline_exercise() {
    let flag = std::env::var("CI_RED_EXERCISE").unwrap_or_default();
    assert_eq!(flag, "red", "intentional red exercise (AC-157)");
}
