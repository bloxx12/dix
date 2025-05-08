mod common;
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use dixlib::store;

// basic benchmarks using the current system
//
// problem: this is not reproducible at all
// since this is very depending on the current
// system and the nature of the system in general
//
// we might want to think about using a copy of the sqlite
// db to benchmark instead to make the results comparable

pub fn bench_get_packages(c: &mut Criterion) {
    c.bench_function("get_packages", |b| {
        b.iter(|| store::get_packages(black_box(common::get_deriv_query())));
    });
}
pub fn bench_get_closure_size(c: &mut Criterion) {
    c.bench_function("get_closure_size", |b| {
        b.iter(|| store::get_closure_size(black_box(common::get_deriv_query())));
    });
}
pub fn bench_get_dependency_graph(c: &mut Criterion) {
    c.bench_function("get_dependency_graph", |b| {
        b.iter(|| store::get_dependency_graph(black_box(common::get_deriv_query())));
    });
}

criterion_group!(
    benches,
    bench_get_packages,
    bench_get_closure_size,
    bench_get_dependency_graph
);
criterion_main!(benches);
