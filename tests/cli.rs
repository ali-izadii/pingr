use std::process::Command;

#[test]
#[ignore = "requires Linux ping socket permission and loopback networking"]
fn sends_ten_probes_then_exits() {
    let output = Command::new(env!("CARGO_BIN_EXE_pingr"))
        .arg("127.0.0.1")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let replies: Vec<_> = stdout
        .lines()
        .filter(|line| line.starts_with("Reply from "))
        .collect();
    assert_eq!(replies.len(), 10, "{stdout}");
    for (index, reply) in replies.iter().enumerate() {
        assert!(
            reply.contains(&format!("sequence={} ", index + 1)),
            "{reply}"
        );
    }
}

#[test]
fn missing_destination_prints_usage_and_exits_two() {
    let output = Command::new(env!("CARGO_BIN_EXE_pingr")).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage: pingr <IPv4 address>"));
}

#[test]
fn invalid_or_extra_arguments_exit_two() {
    for args in [vec!["invalid"], vec!["127.0.0.1", "extra"], vec!["::1"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_pingr"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
