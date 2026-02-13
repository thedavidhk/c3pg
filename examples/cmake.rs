use c3pg::cmake_core::{
    DiscoverMode, LibType, Package, Project, Scope::PRIVATE, Target, TestEntry, TestFramework, TestSuite,
    Value::{Str, Raw},
};

fn main() {
    // You collect sources yourself (no globs).
    let lib_sources = vec!["src/foo.cpp".into(), "src/bar.cpp".into()];
    let app_main = Str("src/main.cpp".into());

    // Library + app
    let lib = Target::library("libexamples", LibType::Static)
        .srcs(lib_sources)
        .link(PRIVATE, Raw("${CONANDEPS_LEGACY}".into())); // Conan deps var

    let app = Target::executable("examples")
        .src(app_main)
        .link(PRIVATE, Str("libexamples".into()));

    // Tests
    let gtest = TestFramework::GoogleTest {
        config_mode: true,
        inline_main_var: "C3PG_GTEST_MAIN".into(),
        inline_main_body: r"
#include <gtest/gtest.h>
int main(int argc, char** argv) {
  ::testing::InitGoogleTest(&argc, argv);
  return RUN_ALL_TESTS();
}
"
        .trim()
        .into(),
        discover_mode: DiscoverMode::PreTest,
    };

    // Create entries per test source you found elsewhere
    let entries = ["tests/test_math.cpp", "tests/test_utils.cpp"]
        .iter()
        .map(|p| {
            let base =
                sanitize_to_c_identifier(p.split('/').next_back().unwrap().split('.').next().unwrap());
            TestEntry {
                exe_name: base.clone(),
                sources: vec![Str(p.to_string()), Raw("${C3PG_GTEST_MAIN}".into())],
                link: vec![Str("libexamples".into()), Raw("gtest::gtest".into())],
                prefix: format!("{base}."),
                cxx_standard: Some(20),
            }
        });

    let suite = entries.fold(
        TestSuite::new_aggregate("examples_tests", gtest),
        c3pg::cmake_core::TestSuite::with_entry,
    );

    // Project (general knobs via set_var + include)
    let txt = Project::new("examples")
        .languages(&["CXX"])
        .set_var("CMAKE_CXX_STANDARD", "20")
        .set_on("CMAKE_CXX_STANDARD_REQUIRED")
        .set_on("CMAKE_EXPORT_COMPILE_COMMANDS")
        .include(Str("${CMAKE_BINARY_DIR}/conandeps_legacy.cmake".into()))
        .find_package(Package {
            name: "GTest".into(),
            required: true,
            config_only: true,
            components: vec![],
        })
        .target(lib)
        .target(app)
        .with_tests(suite)
        .emit().unwrap();

    println!("{txt}");
}

fn sanitize_to_c_identifier(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        out.push(if ch.is_alphanumeric() || ch == '_' {
            ch
        } else {
            '_'
        });
    }
    if out
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
    {
        out.insert(0, '_');
    }
    out
}
