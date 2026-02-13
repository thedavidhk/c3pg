use anyhow::Result;
use std::fmt::Write as _;

#[derive(Debug, Clone)]
pub enum Value {
    Str(String),      // normal argument, auto-quoted if needed
    Raw(String),      // literal: emitted exactly as-is (for ${VAR}, $<GENEX>, etc.)
    List(Vec<Value>), // ;-joined values
    Bracket(String),  // multi-line or raw block, emitted as [=[ … ]=]
}

impl<S: Into<String>> From<S> for Value {
    fn from(s: S) -> Self {
        Value::Str(s.into())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibType {
    Static,
    Shared,
    Interface,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    PRIVATE,
    PUBLIC,
    INTERFACE,
}

#[derive(Debug, Default, Clone)]
pub struct ScopedList {
    privs: Vec<Value>,
    pubs: Vec<Value>,
    ifcs: Vec<Value>,
}
impl ScopedList {
    pub fn push(&mut self, scope: Scope, v: impl Into<Value>) {
        match scope {
            Scope::PRIVATE => self.privs.push(v.into()),
            Scope::PUBLIC => self.pubs.push(v.into()),
            Scope::INTERFACE => self.ifcs.push(v.into()),
        }
    }
    pub fn extend(&mut self, scope: Scope, vs: impl IntoIterator<Item = Value>) {
        for v in vs {
            self.push(scope, v);
        }
    }
}

#[derive(Debug, Clone)]
pub struct Target {
    pub name: String,
    kind: TargetKind,
    sources: Vec<Value>,
    include_dirs: ScopedList,
    compile_defs: ScopedList,
    compile_opts: ScopedList,
    link_libs: ScopedList,
    features: ScopedList,
    properties: Vec<(String, Value)>,
}

#[derive(Debug, Clone)]
pub enum TargetKind {
    Executable,
    Library(LibType),
}

impl Target {
    pub fn executable(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: TargetKind::Executable,
            sources: vec![],
            include_dirs: Default::default(),
            compile_defs: Default::default(),
            compile_opts: Default::default(),
            link_libs: Default::default(),
            features: Default::default(),
            properties: vec![],
        }
    }
    pub fn library(name: impl Into<String>, ty: LibType) -> Self {
        Self {
            kind: TargetKind::Library(ty),
            ..Self::executable(name)
        }
    }

    pub fn src(mut self, path: impl Into<Value>) -> Self {
        self.sources.push(path.into());
        self
    }
    pub fn srcs<I: IntoIterator<Item = Value>>(mut self, paths: I) -> Self {
        self.sources.extend(paths.into_iter().map(Into::into));
        self
    }

    pub fn include(mut self, scope: Scope, dir: impl Into<Value>) -> Self {
        self.include_dirs.push(scope, dir);
        self
    }

    pub fn def(mut self, scope: Scope, def: impl Into<Value>) -> Self {
        self.compile_defs.push(scope, def.into());
        self
    }

    pub fn copt(mut self, scope: Scope, flag: impl Into<Value>) -> Self {
        self.compile_opts.push(scope, flag.into());
        self
    }

    pub fn link(mut self, scope: Scope, lib: impl Into<Value>) -> Self {
        self.link_libs.push(scope, lib.into());
        self
    }

    pub fn prop(mut self, key: impl Into<String>, val: impl Into<Value>) -> Self {
        self.properties.push((key.into(), val.into()));
        self
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub name: String,
    pub version: Option<String>,
    /// e.g. "3.21"
    pub cmake_min: String,
    pub languages: Vec<&'static str>, // e.g. ["C", "CXX"]
    pub targets: Vec<Target>,
    pub packages: Vec<Package>,
    pub settings: Vec<CMakeSetting>,
    pub includes: Vec<Value>,
    pub tests: Option<TestSuite>, // CTest/GTest setup
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub required: bool,
    pub config_only: bool,
    pub components: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct CMakeSetting {
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct TestSuite {
    aggregate_target: String,
    framework: TestFramework,
    entries: Vec<TestEntry>,
}

#[derive(Debug, Clone)]
pub enum TestFramework {
    GoogleTest {
        config_mode: bool,        // find_package(GTest CONFIG REQUIRED)
        inline_main_var: String,  // variable name for the emitted helper file
        inline_main_body: String, // C++ text for custom main
        discover_mode: DiscoverMode,
    },
}

#[derive(Debug, Clone)]
pub enum DiscoverMode {
    PreTest,
    PostBuild,
}

#[derive(Debug, Clone)]
pub struct TestEntry {
    pub exe_name: String,          // already sanitized name
    pub sources: Vec<Value>,       // includes inline_main_var token
    pub link: Vec<Value>,          // libraries to link (e.g., libexamples, gtest::gtest)
    pub prefix: String,            // TEST_PREFIX "name."
    pub cxx_standard: Option<u16>, // emits target_compile_features(... cxx_std_<N>)
}

impl TestSuite {
    pub fn new_aggregate(name: impl Into<String>, framework: TestFramework) -> Self {
        Self {
            aggregate_target: name.into(),
            framework,
            entries: Default::default(),
        }
    }
    pub fn add(mut self, e: TestEntry) -> Self {
        self.entries.push(e);
        self
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn version(mut self, v: impl Into<String>) -> Self {
        self.version = Some(v.into());
        self
    }

    pub fn lang(mut self, langs: &[&'static str]) -> Self {
        self.languages = langs.to_vec();
        self
    }

    pub fn set_var(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.settings.push(CMakeSetting {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    pub fn set_on(mut self, name: impl Into<String>) -> Self {
        self.settings.push(CMakeSetting {
            name: name.into(),
            value: "ON".into(),
        });
        self
    }

    pub fn include(mut self, path: impl Into<Value>) -> Self {
        self.includes.push(path.into());
        self
    }

    pub fn find_package(mut self, package: Package) -> Self {
        self.packages.push(package);
        self
    }

    pub fn target(mut self, t: Target) -> Self {
        self.targets.push(t);
        self
    }

    pub fn with_tests(mut self, suite: TestSuite) -> Self {
        self.tests = Some(suite);
        self
    }

    pub fn languages(mut self, arg: &[&'static str; 1]) -> Self {
        self.languages = arg.into();
        self
    }

    /// Render a complete CMakeLists.txt
    pub fn emit(&self) -> Result<String> {
        let mut out = String::new();

        // ---- header ----
        writeln!(
            &mut out,
            "cmake_minimum_required(VERSION {})",
            self.cmake_min
        )?;
        if self.languages.is_empty() {
            writeln!(&mut out, "project({})", q(&self.name))?;
        } else {
            writeln!(
                &mut out,
                "project({} LANGUAGES {})\n",
                q(&self.name),
                self.languages.join(" ")
            )?;
        }

        // ---- generic settings (set(VAR VALUE)) ----
        for CMakeSetting { name, value } in &self.settings {
            write!(&mut out, "set(")?;
            emit_val(&mut out, Value::Raw(name.clone()))?;
            write!(&mut out, " ")?;
            emit_val(&mut out, value.clone())?;
            writeln!(&mut out, ")")?;
        }
        writeln!(&mut out, "")?;

        // ---- includes ----
        for inc in &self.includes {
            write!(&mut out, "include(")?;
            emit_val(&mut out, inc.clone())?;
            writeln!(&mut out, ")")?;
        }
        writeln!(&mut out, "")?;

        // ---- find_package ----
        for p in &self.packages {
            write!(&mut out, "find_package({}", p.name)?;
            if p.config_only {
                write!(&mut out, " CONFIG")?;
            }
            if p.required {
                write!(&mut out, " REQUIRED")?;
            }
            if !p.components.is_empty() {
                write!(&mut out, " COMPONENTS")?;
                for c in &p.components {
                    write!(&mut out, " {}", c)?;
                }
            }
            writeln!(&mut out, ")")?;
        }
        writeln!(&mut out, "")?;

        // ---- targets ----
        for t in &self.targets {
            // minimal validation
            if matches!(t.kind, TargetKind::Library(LibType::Interface)) && !t.sources.is_empty() {
                return Err(anyhow::anyhow!(
                    "INTERFACE library '{}' cannot have sources",
                    t.name
                ));
            }

            match t.kind {
                TargetKind::Executable => {
                    write!(&mut out, "add_executable({}", t.name)?;
                    for s in &t.sources {
                        write!(&mut out, " ")?;
                        emit_val(&mut out, s.clone())?;
                    }
                    writeln!(&mut out, ")")?;
                }
                TargetKind::Library(kind) => {
                    write!(&mut out, "add_library({}", t.name)?;
                    write!(
                        &mut out,
                        " {}",
                        match kind {
                            LibType::Static => "STATIC",
                            LibType::Shared => "SHARED",
                            LibType::Interface => "INTERFACE",
                        }
                    )?;
                    if !matches!(kind, LibType::Interface) {
                        for s in &t.sources {
                            write!(&mut out, " ")?;
                            emit_val(&mut out, s.clone())?;
                        }
                    }
                    writeln!(&mut out, ")")?;
                }
            }

            // target_* families via a single generic emitter
            emit_scoped_list(
                &mut out,
                "target_include_directories",
                &t.name,
                &t.include_dirs,
            )?;
            emit_scoped_list(
                &mut out,
                "target_compile_definitions",
                &t.name,
                &t.compile_defs,
            )?;
            emit_scoped_list(&mut out, "target_compile_options", &t.name, &t.compile_opts)?;
            emit_scoped_list(&mut out, "target_link_libraries", &t.name, &t.link_libs)?;
            emit_scoped_list(&mut out, "target_compile_features", &t.name, &t.features)?;

            // target properties
            for (k, v) in &t.properties {
                write!(
                    &mut out,
                    "set_target_properties({} PROPERTIES {} ",
                    t.name, k
                )?;
                emit_val(&mut out, v.clone())?;
                writeln!(&mut out, ")")?;
            }

            writeln!(&mut out)?;
        }

        // ---- tests (optional) ----
        if let Some(ts) = &self.tests {
            // CTest boilerplate
            writeln!(&mut out, "include(CTest)")?;
            writeln!(&mut out, "enable_testing()")?;

            // Currently only GoogleTest is modeled
            match &ts.framework {
                TestFramework::GoogleTest {
                    config_mode,
                    inline_main_var,
                    inline_main_body,
                    discover_mode,
                } => {
                    // find_package(GTest ...)
                    write!(&mut out, "find_package(GTest")?;
                    if *config_mode {
                        write!(&mut out, " CONFIG")?;
                    }
                    writeln!(&mut out, " REQUIRED)")?;

                    // Inline gtest main
                    write!(&mut out, "set(")?;
                    emit_val(&mut out, Value::Raw(inline_main_var.clone()))?;
                    write!(&mut out, " ")?;
                    emit_val(
                        &mut out,
                        Value::Raw("${CMAKE_CURRENT_BINARY_DIR}/_gtest_main.cpp".into()),
                    )?;
                    writeln!(&mut out, ")")?;

                    write!(&mut out, "file(WRITE ")?;
                    emit_val(&mut out, Value::Raw(format!("${{{}}}", inline_main_var)))?;
                    write!(&mut out, " ")?;
                    emit_val(&mut out, Value::Bracket(inline_main_body.clone()))?;
                    writeln!(&mut out, ")")?;

                    // Aggregate phony target
                    writeln!(&mut out, "add_custom_target({})", ts.aggregate_target)?;

                    // Only include once
                    writeln!(&mut out, "include(GoogleTest)")?;

                    // Each entry becomes an executable + discover + dependency
                    for e in &ts.entries {
                        // add_executable
                        write!(&mut out, "add_executable({}", e.exe_name)?;
                        for s in &e.sources {
                            write!(&mut out, " ")?;
                            emit_val(&mut out, s.clone())?;
                        }
                        writeln!(&mut out, ")")?;

                        // target_link_libraries
                        if !e.link.is_empty() {
                            write!(&mut out, "target_link_libraries({} PRIVATE", e.exe_name)?;
                            for l in &e.link {
                                write!(&mut out, " ")?;
                                emit_val(&mut out, l.clone())?;
                            }
                            writeln!(&mut out, ")")?;
                        }

                        // cxx_std_N via target_compile_features
                        if let Some(std) = e.cxx_standard {
                            writeln!(
                                &mut out,
                                "target_compile_features({} PRIVATE cxx_std_{})",
                                e.exe_name, std
                            )?;
                        }

                        // gtest_discover_tests
                        write!(&mut out, "gtest_discover_tests({}", e.exe_name)?;
                        if !e.prefix.is_empty() {
                            write!(&mut out, " TEST_PREFIX ")?;
                            emit_val(&mut out, Value::Str(e.prefix.clone()))?;
                        }
                        write!(
                            &mut out,
                            " DISCOVERY_MODE {}",
                            match discover_mode {
                                DiscoverMode::PreTest => "PRE_TEST",
                                DiscoverMode::PostBuild => "POST_BUILD",
                            }
                        )?;
                        writeln!(&mut out, ")")?;

                        // add_dependencies(aggregate exe)
                        writeln!(
                            &mut out,
                            "add_dependencies({} {})",
                            ts.aggregate_target, e.exe_name
                        )?;
                        writeln!(&mut out)?;
                    }
                }
            }
        }

        Ok(out)
    }
}

/* ----------------- helpers (internal) ----------------- */

fn emit_scoped_list(
    out: &mut String,
    cmd: &str,
    target: &str,
    list: &ScopedList,
) -> anyhow::Result<()> {
    let empty = list.privs.is_empty() && list.pubs.is_empty() && list.ifcs.is_empty();
    if empty {
        return Ok(());
    }

    writeln!(out, "{}({}", cmd, target)?;
    if !list.privs.is_empty() {
        writeln!(out, "    PRIVATE")?;
        for v in &list.privs {
            write!(out, "        ")?;
            emit_val(out, v.clone())?;
            writeln!(out)?;
        }
    }
    if !list.pubs.is_empty() {
        writeln!(out, "    PUBLIC")?;
        for v in &list.pubs {
            write!(out, "        ")?;
            emit_val(out, v.clone())?;
            writeln!(out)?;
        }
    }
    if !list.ifcs.is_empty() {
        writeln!(out, "    INTERFACE")?;
        for v in &list.ifcs {
            write!(out, "        ")?;
            emit_val(out, v.clone())?;
            writeln!(out)?;
        }
    }
    writeln!(out, ")")?;
    Ok(())
}

fn emit_val(out: &mut String, v: Value) -> anyhow::Result<()> {
    match v {
        Value::Str(s) => {
            write!(out, "{}", q(&s))?;
        }
        Value::Raw(s) => {
            write!(out, "{}", s)?;
        }
        Value::List(xs) => {
            // ;-joined list; escape semicolons in strings
            let mut first = true;
            for x in xs {
                if !first {
                    write!(out, ";")?;
                }
                match x {
                    Value::Str(s) => write!(out, "{}", s.replace(';', "\\;"))?,
                    Value::Raw(s) => write!(out, "{}", s)?,
                    Value::List(_) => {
                        anyhow::bail!("nested lists are not supported in Value::List")
                    }
                    Value::Bracket(_) => {
                        anyhow::bail!("bracket args are only valid as standalone payloads")
                    }
                }
                first = false;
            }
        }
        Value::Bracket(body) => {
            // raw multi-line block using [=[ ... ]=]
            write!(out, "[=[\n{}]=]", body)?;
        }
    }
    Ok(())
}

fn q(s: &str) -> String {
    // quote only if necessary; escape " and \
    let needs = s
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, ';' | '#' | '(' | ')' | '"'));
    if !needs {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: Default::default(),
            version: Some("0.1.0".to_string()),
            cmake_min: "3.21".to_string(),
            languages: vec!["CXX"],
            targets: Default::default(),
            packages: Default::default(),
            settings: Default::default(),
            tests: Default::default(),
            includes: Default::default(),
        }
    }
}
