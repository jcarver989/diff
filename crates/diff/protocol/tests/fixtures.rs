use clankerdiff_protocol::{ReviewOutcome, parse_response};

#[test]
fn version_one_golden_responses_remain_compatible() {
    let fixtures = [
        (
            include_bytes!("fixtures/diff-approved-v1.json").as_slice(),
            ReviewOutcome::Approved,
        ),
        (
            include_bytes!("fixtures/diff-changes-requested-v1.json").as_slice(),
            ReviewOutcome::ChangesRequested,
        ),
        (
            include_bytes!("fixtures/diff-cancelled-v1.json").as_slice(),
            ReviewOutcome::Cancelled,
        ),
        (
            include_bytes!("fixtures/markdown-approved-v1.json").as_slice(),
            ReviewOutcome::Approved,
        ),
        (
            include_bytes!("fixtures/markdown-changes-requested-v1.json").as_slice(),
            ReviewOutcome::ChangesRequested,
        ),
        (
            include_bytes!("fixtures/markdown-cancelled-v1.json").as_slice(),
            ReviewOutcome::Cancelled,
        ),
    ];

    for (fixture, expected) in fixtures {
        let response = parse_response(fixture).unwrap();
        assert_eq!(response.outcome(), expected);
    }
}
