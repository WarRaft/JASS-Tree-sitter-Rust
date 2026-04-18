#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::lng::jass::builder::project::{ProjectAst, ProjectFile};
    use crate::lng::jass::builder::PipelineMode;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use url::Url;

    #[test]
    fn analyze_project_renames_shadowed_function_locals_in_exact_cunt_ret_case() {
        let source = r#"function Cunt_ret takes nothing returns unit
    integer Cunt_ret = 1
endfunction

function Cunt takes nothing returns unit
    integer Cunt = 3
    unit A = GetTriggerUnit()
    A = Cunt_ret()
    return A
endfunction

integer Cunt_ret = 3
"#;

        let project = ProjectAst {
            out_path: PathBuf::from("/tmp/war3map.j"),
            files: vec![ProjectFile {
                uri: Url::parse("file:///tmp/test.j").expect("valid file url"),
                source: source.to_string(),
                is_frozen: false,
                function_callees: HashMap::new(),
            }],
        };

        let plan = analyze_project(&project, PipelineMode::Build, false);
        let out = render_plan(&plan, &project.out_path);

        assert!(
            out.contains("globals\n    unit Cunt_ret2 = null\n    integer Cunt_ret1 = 3\nendglobals"),
            "unexpected globals block:\n{out}"
        );
        assert!(
            out.contains(
                "function Cunt_ret takes nothing returns unit\n    local integer Cunt_ret1 = 1\nendfunction"
            ),
            "expected shadowed local in Cunt_ret to be renamed:\n{out}"
        );
        assert!(
            out.contains(
                "function Cunt takes nothing returns unit\n    local integer Cunt1 = 3\n    local unit A = GetTriggerUnit()\n    set A = Cunt_ret()\n    set Cunt_ret2 = A\n    set A = null\n    return Cunt_ret2\nendfunction"
            ),
            "expected shadowed local in Cunt to be renamed:\n{out}"
        );
    }

    #[test]
    fn analyze_project_allows_local_to_shadow_global_variable() {
        let source = r#"integer Shared = 3

function UseLocal takes nothing returns integer
    local integer Shared = 7
    return Shared
endfunction
"#;

        let project = ProjectAst {
            out_path: PathBuf::from("/tmp/war3map.j"),
            files: vec![ProjectFile {
                uri: Url::parse("file:///tmp/test_local_shadow_global.j").expect("valid file url"),
                source: source.to_string(),
                is_frozen: false,
                function_callees: HashMap::new(),
            }],
        };

        let plan = analyze_project(&project, PipelineMode::Build, false);
        let out = render_plan(&plan, &project.out_path);

        assert!(
            out.contains("globals\n    integer Shared = 3\nendglobals"),
            "global must remain unchanged when no function has that name:\n{out}"
        );
        assert!(
            out.contains("function UseLocal takes nothing returns integer\n    local integer Shared = 7\n    return Shared\nendfunction"),
            "local should be allowed to shadow the global:\n{out}"
        );
    }

    #[test]
    fn analyze_project_allows_arg_to_shadow_global_variable() {
        let source = r#"integer Shared = 3

function UseArg takes integer Shared returns integer
    return Shared
endfunction
"#;

        let project = ProjectAst {
            out_path: PathBuf::from("/tmp/war3map.j"),
            files: vec![ProjectFile {
                uri: Url::parse("file:///tmp/test_arg_shadow_global.j").expect("valid file url"),
                source: source.to_string(),
                is_frozen: false,
                function_callees: HashMap::new(),
            }],
        };

        let plan = analyze_project(&project, PipelineMode::Build, false);
        let out = render_plan(&plan, &project.out_path);

        assert!(
            out.contains("globals\n    integer Shared = 3\nendglobals"),
            "global must remain unchanged when only the arg shares its name:\n{out}"
        );
        assert!(
            out.contains("function UseArg takes integer Shared returns integer\n    return Shared\nendfunction"),
            "argument should be allowed to shadow the global:\n{out}"
        );
    }

    #[test]
    fn analyze_project_rewrites_global_reads_and_writes_after_function_collision() {
        let source = r#"function Cunt_ret takes nothing returns integer
    return 1
endfunction

function ReadGlobal takes nothing returns integer
    return Cunt_ret
endfunction

function WriteGlobal takes nothing returns nothing
    set Cunt_ret = 7
endfunction

integer Cunt_ret = 3
"#;

        let project = ProjectAst {
            out_path: PathBuf::from("/tmp/war3map.j"),
            files: vec![ProjectFile {
                uri: Url::parse("file:///tmp/test_globals.j").expect("valid file url"),
                source: source.to_string(),
                is_frozen: false,
                function_callees: HashMap::new(),
            }],
        };

        let plan = analyze_project(&project, PipelineMode::Build, false);
        let out = render_plan(&plan, &project.out_path);

        assert!(
            out.contains("globals\n    integer Cunt_ret1 = 3\nendglobals"),
            "expected renamed global declaration:\n{out}"
        );
        assert!(
            out.contains("function ReadGlobal takes nothing returns integer\n    return Cunt_ret1\nendfunction"),
            "expected global read to use renamed global:\n{out}"
        );
        assert!(
            out.contains("function WriteGlobal takes nothing returns nothing\n    set Cunt_ret1 = 7\nendfunction"),
            "expected global write to use renamed global:\n{out}"
        );
        assert!(
            out.contains("function Cunt_ret takes nothing returns integer\n    return 1\nendfunction"),
            "expected function name to remain unchanged:\n{out}"
        );
    }
}

