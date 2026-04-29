use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use ledger_core::AliasMatcher;

fn build_matcher(n_rules: usize) -> AliasMatcher {
    let mut m = AliasMatcher::new();
    for i in 0..n_rules {
        // Mix of rule shapes so all index paths are exercised:
        match i % 5 {
            // prefix-indexed
            0 => m
                .register(
                    &format!("canonical/prefix/{i}/{{id}}"),
                    &format!("/api/v{i}/{{id}}"),
                )
                .unwrap(),
            // suffix-indexed
            1 => m
                .register(
                    &format!("canonical/suffix/{i}/{{name}}"),
                    &format!("{{name}}.ext{i}"),
                )
                .unwrap(),
            // contains-indexed (interior literal)
            2 => m
                .register(
                    &format!("canonical/contains/{i}/{{a}}/{{b}}"),
                    &format!("{{a}}-mid{i}-{{b}}"),
                )
                .unwrap(),
            // exact (no placeholders)
            3 => m
                .register(
                    &format!("canonical/exact/{i}"),
                    &format!("/static/route/{i}"),
                )
                .unwrap(),
            // fallback (catch-all shape, but with a unique prefix to avoid
            // swallowing everything — placed last so it doesn't shadow others)
            _ => m
                .register(
                    &format!("canonical/fallback/{i}/{{x}}"),
                    &format!("{{x}}"),
                )
                .unwrap(),
        };
    }
    m
}

fn bench_lookup_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_hit");
    for n in [10, 100, 1_000] {
        let m = build_matcher(n);

        // Hit a prefix rule near the end
        let rule_idx = (n - 1) / 5 * 5; // last prefix rule
        let input = format!("/api/v{rule_idx}/some-id");

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let r = m.lookup(black_box(&input));
                debug_assert!(r.is_some());
                r
            });
        });
    }
    group.finish();
}

fn bench_lookup_miss(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_miss");
    for n in [10, 100, 1_000] {
        let m = build_matcher(n);
        let input = "/totally/unknown/path/that/matches/nothing";

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let r = m.lookup(black_box(input));
                debug_assert!(r.is_none());
                r
            });
        });
    }
    group.finish();
}

fn bench_lookup_exact_hit(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup_exact_hit");
    for n in [10, 100, 1_000] {
        let m = build_matcher(n);
        let rule_idx = 3; // first exact rule
        let input = format!("/static/route/{rule_idx}");

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| {
                let r = m.lookup(black_box(&input));
                debug_assert!(r.is_some());
                r
            });
        });
    }
    group.finish();
}

fn bench_resolve_segment_style(c: &mut Criterion) {
    let mut group = c.benchmark_group("resolve_segment_style");
    for n in [10, 100, 1_000] {
        let mut m = AliasMatcher::new();
        for i in 0..n {
            m.register(
                &format!("user/{{uid}}/to_pay/{i}"),
                &format!("sale/{i}/receivables/{{uid}}"),
            )
            .unwrap();
        }
        // Hit the last rule
        let input = format!("sale/{}/receivables/42", n - 1);

        group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
            b.iter(|| m.resolve(black_box(&input)));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_lookup_hit,
    bench_lookup_miss,
    bench_lookup_exact_hit,
    bench_resolve_segment_style,
);
criterion_main!(benches);
