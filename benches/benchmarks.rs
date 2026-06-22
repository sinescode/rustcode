//! Benchmarks for critical BlazeCode paths.
//!
//! Run with: `cargo bench`

use criterion::{black_box, criterion_group, criterion_main, Criterion};

// ── ID generation ───────────────────────────────────────────────────────

fn bench_id_creation(c: &mut Criterion) {
    c.bench_function("id::ascending", |b| {
        b.iter(|| {
            blazecode_core::id::ascending(
                blazecode_core::id::IdPrefix::Message,
                None,
            )
        })
    });
}

fn bench_id_creation_with_prefix(c: &mut Criterion) {
    c.bench_function("id::ascending_with_prefix", |b| {
        b.iter(|| {
            blazecode_core::id::ascending(
                blazecode_core::id::IdPrefix::Session,
                Some("test_prefix"),
            )
        })
    });
}

// ── Wildcard matching ──────────────────────────────────────────────────

fn bench_wildcard_exact(c: &mut Criterion) {
    c.bench_function("wildcard::exact_match", |b| {
        b.iter(|| {
            blazecode_core::permission::wildcard_match(
                black_box("bash"),
                black_box("bash"),
            )
        })
    });
}

fn bench_wildcard_star(c: &mut Criterion) {
    c.bench_function("wildcard::star_match", |b| {
        b.iter(|| {
            blazecode_core::permission::wildcard_match(
                black_box("src/main.rs"),
                black_box("*.rs"),
            )
        })
    });
}

fn bench_wildcard_double_star(c: &mut Criterion) {
    c.bench_function("wildcard::double_star", |b| {
        b.iter(|| {
            blazecode_core::permission::wildcard_match(
                black_box("src/components/Button.tsx"),
                black_box("**/*.tsx"),
            )
        })
    });
}

// ── Permission evaluation ──────────────────────────────────────────────

fn bench_permission_evaluate(c: &mut Criterion) {
    use blazecode_core::permission::{evaluate, PermissionAction, PermissionRule};
    let rules = vec![
        PermissionRule { permission: "bash".into(), pattern: "*".into(), action: PermissionAction::Allow },
        PermissionRule { permission: "read".into(), pattern: "/etc/*".into(), action: PermissionAction::Deny },
        PermissionRule { permission: "read".into(), pattern: "/home/*".into(), action: PermissionAction::Allow },
        PermissionRule { permission: "write".into(), pattern: "*".into(), action: PermissionAction::Ask },
    ];
    c.bench_function("permission::evaluate", |b| {
        b.iter(|| {
            evaluate(black_box("read"), black_box("/etc/passwd"), &[&rules])
        })
    });
}

// ── Serde serialization ────────────────────────────────────────────────

fn bench_serde_session_info(c: &mut Criterion) {
    use blazecode_core::session::SessionInfo;
    let info = SessionInfo::default();
    c.bench_function("serde::session_info_serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&info)))
    });
}

fn bench_serde_session_info_roundtrip(c: &mut Criterion) {
    use blazecode_core::session::SessionInfo;
    let info = SessionInfo::default();
    let json = serde_json::to_string(&info).unwrap();
    c.bench_function("serde::session_info_roundtrip", |b| {
        b.iter(|| serde_json::from_str::<SessionInfo>(black_box(&json)))
    });
}

// ── Config variable substitution ───────────────────────────────────────

fn bench_substitute_variables(c: &mut Criterion) {
    let text = "Hello {env:USER}, your home is {env:HOME}";
    let dir = std::path::Path::new("/tmp");
    c.bench_function("config::substitute_variables", |b| {
        b.iter(|| {
            blazecode_core::config::substitute_variables(
                black_box(text),
                black_box(dir),
                None,
            )
        })
    });
}

// ── Shell parsing ──────────────────────────────────────────────────────

fn bench_shell_parse_simple(c: &mut Criterion) {
    let parser = blazecode_core::shell_parser::ShellParser::new();
    c.bench_function("shell::parse_simple", |b| {
        b.iter(|| parser.parse(black_box("ls -la")))
    });
}

fn bench_shell_parse_complex(c: &mut Criterion) {
    let parser = blazecode_core::shell_parser::ShellParser::new();
    c.bench_function("shell::parse_complex", |b| {
        b.iter(|| parser.parse(black_box(
            "find /home/user/projects -name '*.rs' -exec grep -l 'fn main' {} \\; | head -20"
        )))
    });
}

// ── Tool truncation ────────────────────────────────────────────────────

fn bench_truncate_small(c: &mut Criterion) {
    let svc = blazecode_core::truncate::TruncateService::new();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let text = "Hello, world!";
    c.bench_function("truncate::small", |b| {
        b.iter(|| {
            runtime.block_on(svc.truncate(black_box(text), "ses_001", "call_001"))
        })
    });
}

fn bench_truncate_large(c: &mut Criterion) {
    let svc = blazecode_core::truncate::TruncateService::new();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let text = "line\n".repeat(10_000);
    c.bench_function("truncate::large_10k_lines", |b| {
        b.iter(|| {
            runtime.block_on(svc.truncate(black_box(&text), "ses_001", "call_002"))
        })
    });
}

criterion_group!(
    benches,
    bench_id_creation,
    bench_id_creation_with_prefix,
    bench_wildcard_exact,
    bench_wildcard_star,
    bench_wildcard_double_star,
    bench_permission_evaluate,
    bench_serde_session_info,
    bench_serde_session_info_roundtrip,
    bench_substitute_variables,
    bench_shell_parse_simple,
    bench_shell_parse_complex,
    bench_truncate_small,
    bench_truncate_large,
);
criterion_main!(benches);
