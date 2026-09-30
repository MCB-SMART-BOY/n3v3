//! Process-plan lowering for the existing process runtime values.
//! 现有进程运行时值的进程计划降级。

use n3v3_common::{ProcessPlan, ProcessRedirect, ProcessStage, ProcessStream};
use n3v3_eval::value::{CommandValue, PipelineValue, RedirectStream, RedirectValue};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn builtins() -> Vec<(&'static str, n3v3_eval::value::Value)> {
    vec![]
}

pub(crate) fn lower_command_plan(command: &CommandValue) -> ProcessPlan {
    ProcessPlan::new(vec![lower_stage(command)], Vec::new())
}

pub(crate) fn lower_pipeline_plan(
    pipeline: &PipelineValue,
    fn_name: &str,
) -> Result<ProcessPlan, String> {
    super::validate_pipeline_command_topology(pipeline.commands(), fn_name)?;
    Ok(ProcessPlan::new(
        pipeline
            .commands()
            .iter()
            .map(|command| lower_stage(command))
            .collect(),
        lower_redirects(pipeline.redirects()),
    ))
}

fn lower_stage(command: &CommandValue) -> ProcessStage {
    let env: BTreeMap<String, String> = command
        .env()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    ProcessStage::new(
        command.program().to_string(),
        command.args().to_vec(),
        command.cwd().map(str::to_owned),
        command.stdin().map(str::to_owned),
        env,
        lower_redirects(command.redirects()),
    )
}

fn lower_redirects(redirects: &[RedirectValue]) -> Vec<ProcessRedirect> {
    redirects.iter().map(lower_redirect).collect()
}

fn lower_redirect(redirect: &RedirectValue) -> ProcessRedirect {
    let stream = match redirect.stream() {
        RedirectStream::Stdin => ProcessStream::Stdin,
        RedirectStream::Stdout => ProcessStream::Stdout,
        RedirectStream::Stderr => ProcessStream::Stderr,
    };
    ProcessRedirect::new(stream, PathBuf::from(redirect.path()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::rc::Rc;

    #[test]
    fn lower_command_plan_preserves_stage_fields() {
        let command = CommandValue::new_with_options_and_redirects(
            "printf",
            vec!["n3v3".to_string()],
            Some("/tmp".to_string()),
            Some("input".to_string()),
            HashMap::from([("LANG".to_string(), "C".to_string())]),
            vec![RedirectValue::stderr_path("errors.txt")],
        );

        let plan = lower_command_plan(&command);
        let stage = plan.stages().first().expect("command stage");

        assert_eq!(stage.program(), "printf");
        assert_eq!(stage.args(), ["n3v3"]);
        assert_eq!(stage.cwd(), Some("/tmp"));
        assert_eq!(stage.stdin(), Some("input"));
        assert_eq!(stage.env().get("LANG"), Some(&"C".to_string()));
        assert_eq!(stage.redirects().len(), 1);
        assert_eq!(stage.redirects()[0].stream(), ProcessStream::Stderr);
        assert_eq!(stage.redirects()[0].path(), PathBuf::from("errors.txt"));
        assert!(plan.boundary_redirects().is_empty());
    }

    #[test]
    fn lower_pipeline_plan_preserves_order_and_boundary_redirects() {
        let first = Rc::new(CommandValue::new("printf", vec!["n3v3".to_string()]));
        let second = Rc::new(CommandValue::new("cat", Vec::new()));
        let pipeline = PipelineValue::new_with_redirects(
            vec![first, second],
            vec![RedirectValue::stdout_path("output.txt")],
        );

        let plan = lower_pipeline_plan(&pipeline, "io.execPipeline").expect("pipeline plan");

        assert_eq!(
            plan.stages()
                .iter()
                .map(ProcessStage::program)
                .collect::<Vec<_>>(),
            ["printf", "cat"]
        );
        assert_eq!(plan.boundary_redirects().len(), 1);
        assert_eq!(plan.boundary_redirects()[0].stream(), ProcessStream::Stdout);
        assert_eq!(
            plan.boundary_redirects()[0].path(),
            PathBuf::from("output.txt")
        );
    }

    #[test]
    fn lower_pipeline_plan_rejects_empty_pipeline_with_context() {
        let pipeline = PipelineValue::new(Vec::new());

        let error = lower_pipeline_plan(&pipeline, "io.execPipeline")
            .expect_err("empty pipeline should be rejected");

        assert_eq!(error, "io.execPipeline: requires a non-empty List<Command>");
    }
}
