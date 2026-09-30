use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat};

mod builtins;
mod fetch_map_set;
mod io;
mod list;
mod math;
mod option_result;
mod path;
mod string;

type CompletionSpec = (&'static str, &'static str, &'static str, &'static str);

pub(crate) fn completion_items() -> Vec<CompletionItem> {
    let mut specs = io::specs();
    specs.extend(math::specs());
    specs.extend(builtins::specs());
    specs.extend(fetch_map_set::specs());
    specs.extend(list::specs());
    specs.extend(string::specs());
    specs.extend(path::specs());
    specs.extend(option_result::specs());

    specs
        .into_iter()
        .map(|(label, detail, snippet, ret_type)| CompletionItem {
            label: label.to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(format!("{} -> {}", detail, ret_type)),
            insert_text: Some(snippet.to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{fetch_map_set, io, list, math, option_result, path, string};

    #[test]
    fn test_list_stdlib_completions_match_real_surface() {
        let labels: Vec<_> = list::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"list.empty"));
        assert!(labels.contains(&"list.singleton"));
        assert!(labels.contains(&"list.isEmpty"));
        assert!(labels.contains(&"list.foldRight"));
        assert!(labels.contains(&"list.zip"));
    }

    #[test]
    fn test_list_stdlib_completions_omit_stale_surface_entries() {
        let labels: Vec<_> = list::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(!labels.contains(&"list.concat"));
        assert!(!labels.contains(&"list.elem"));
    }

    #[test]
    fn test_io_stdlib_completions_match_real_surface() {
        let labels: Vec<_> = io::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"io.readFilePath"));
        assert!(labels.contains(&"io.pathExistsPath"));
        assert!(labels.contains(&"io.currentDirPath"));
        assert!(labels.contains(&"io.homeDirPath"));
        assert!(labels.contains(&"io.command"));
        assert!(labels.contains(&"io.commandWith"));
        assert!(labels.contains(&"io.commandWithRedirects"));
        assert!(labels.contains(&"io.execCommand"));
        assert!(labels.contains(&"io.pipeline"));
        assert!(labels.contains(&"io.pipelineWithRedirects"));
        assert!(labels.contains(&"io.execPipeline"));
        assert!(labels.contains(&"io.redirectStdoutPath"));
        assert!(labels.contains(&"io.redirectStderrPath"));
        assert!(labels.contains(&"io.redirectStdinPath"));
        assert!(labels.contains(&"io.taskCommand"));
        assert!(labels.contains(&"io.taskPipeline"));
        assert!(labels.contains(&"io.awaitTask"));
        assert!(labels.contains(&"io.awaitTasks"));
        assert!(labels.contains(&"io.processSuccess"));
        assert!(labels.contains(&"io.processStdout"));
        assert!(labels.contains(&"io.processCode"));
        assert!(labels.contains(&"io.processStderr"));
        // Stream<T> APIs
        assert!(labels.contains(&"io.streamList"));
        assert!(labels.contains(&"io.streamLines"));
        assert!(labels.contains(&"io.streamCommand"));
        assert!(labels.contains(&"io.streamBytes"));
        assert!(labels.contains(&"io.streamMap"));
        assert!(labels.contains(&"io.streamFilter"));
        assert!(labels.contains(&"io.streamTake"));
        assert!(labels.contains(&"io.streamDrop"));
        assert!(labels.contains(&"io.streamCollect"));
        assert!(labels.contains(&"io.streamPipe"));
        assert!(labels.contains(&"io.streamForEach"));
        assert!(labels.contains(&"io.streamFold"));
        assert!(labels.contains(&"io.streamWithTimeout"));
        // Task cancel / await APIs
        assert!(labels.contains(&"io.cancel"));
        assert!(labels.contains(&"io.awaitAny"));
        assert!(labels.contains(&"io.awaitTaskWithTimeout"));
        // Reactive APIs
        assert!(labels.contains(&"io.liveNext"));
        assert!(labels.contains(&"io.liveCurrent"));
        assert!(labels.contains(&"io.liveCancel"));
        // TTY APIs
        assert!(labels.contains(&"io.isTTY"));
        assert!(labels.contains(&"io.terminalSize"));
        assert!(labels.contains(&"io.setRawMode"));
        assert!(labels.contains(&"io.resetTerminal"));
        assert!(labels.contains(&"io.readKey"));
        // Job control APIs
        assert!(labels.contains(&"io.jobs"));
        assert!(labels.contains(&"io.waitAnyJob"));
        // Other I/O
        assert!(labels.contains(&"io.kill"));
        assert!(labels.contains(&"io.args"));
    }

    #[test]
    fn test_io_stdlib_completions_omit_removed_compat_wrappers() {
        let labels: Vec<_> = io::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(!labels.contains(&"io.exec"));
        assert!(!labels.contains(&"io.execWith"));
        assert!(!labels.contains(&"io.execShell"));
        assert!(!labels.contains(&"io.execResult"));
        assert!(!labels.contains(&"io.execShellResult"));
        assert!(!labels.contains(&"io.execWithResult"));
        assert!(!labels.contains(&"io.execCommandWithRedirects"));
        assert!(!labels.contains(&"io.execPipelineWithRedirects"));
    }

    #[test]
    fn test_option_result_stdlib_completions_match_real_surface() {
        let labels: Vec<_> = option_result::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"option.some"));
        assert!(labels.contains(&"option.none"));
        assert!(labels.contains(&"option.is_some"));
        assert!(labels.contains(&"option.is_none"));
        assert!(labels.contains(&"option.unwrap"));
        assert!(labels.contains(&"option.unwrap_or"));
        assert!(labels.contains(&"result.ok"));
        assert!(labels.contains(&"result.err"));
        assert!(labels.contains(&"result.is_ok"));
        assert!(labels.contains(&"result.is_err"));
        assert!(labels.contains(&"result.unwrap"));
        assert!(labels.contains(&"result.unwrap_err"));
    }

    #[test]
    fn test_option_result_stdlib_completions_omit_stale_surface_entries() {
        let labels: Vec<_> = option_result::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(!labels.contains(&"option.isSome"));
        assert!(!labels.contains(&"option.isNone"));
        assert!(!labels.contains(&"option.unwrapOr"));
        assert!(!labels.contains(&"option.map"));
        assert!(!labels.contains(&"result.isOk"));
        assert!(!labels.contains(&"result.isErr"));
        assert!(!labels.contains(&"result.map"));
    }

    #[test]
    fn test_string_stdlib_completions_match_real_surface() {
        let labels: Vec<_> = string::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"string.len"));
        assert!(labels.contains(&"string.chars"));
        assert!(labels.contains(&"string.split"));
        assert!(labels.contains(&"string.join"));
        assert!(labels.contains(&"string.trim"));
        assert!(labels.contains(&"string.upper"));
        assert!(labels.contains(&"string.lower"));
        assert!(labels.contains(&"string.contains"));
        assert!(labels.contains(&"string.startsWith"));
        assert!(labels.contains(&"string.endsWith"));
        assert!(labels.contains(&"string.replace"));
        assert!(labels.contains(&"string.substring"));
        assert!(labels.contains(&"string.isEmpty"));
        assert!(labels.contains(&"string.repeat"));
        assert!(labels.contains(&"string.lines"));
    }

    #[test]
    fn test_string_stdlib_completions_omit_stale_surface_entries() {
        let labels: Vec<_> = string::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(!labels.contains(&"string.concat"));
    }

    #[test]
    fn test_fetch_map_set_stdlib_completions_match_real_surface() {
        let labels: Vec<_> = fetch_map_set::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"fetch.path"));
        assert!(labels.contains(&"fetch.pathWithHash"));
        assert!(labels.contains(&"fetch.url"));
        assert!(labels.contains(&"fetch.urlWithHash"));
        assert!(labels.contains(&"fetch.git"));
        assert!(labels.contains(&"fetch.gitWithHash"));
        assert!(labels.contains(&"Map.empty"));
        assert!(labels.contains(&"Map.getWithDefault"));
        assert!(labels.contains(&"Map.size"));
        assert!(labels.contains(&"Map.isEmpty"));
        assert!(labels.contains(&"Map.values"));
        assert!(labels.contains(&"Map.insert"));
        assert!(labels.contains(&"Map.remove"));
        assert!(labels.contains(&"Map.union"));
        assert!(labels.contains(&"Map.intersection"));
        assert!(labels.contains(&"Map.difference"));
        assert!(labels.contains(&"Set.empty"));
        assert!(labels.contains(&"Set.size"));
        assert!(labels.contains(&"Set.isEmpty"));
        assert!(labels.contains(&"Set.insert"));
        assert!(labels.contains(&"Set.remove"));
        assert!(labels.contains(&"Set.union"));
        assert!(labels.contains(&"Set.intersection"));
        assert!(labels.contains(&"Set.difference"));
        assert!(labels.contains(&"Set.symmetricDifference"));
        assert!(labels.contains(&"Set.isSubset"));
        assert!(labels.contains(&"Set.isSuperset"));
        assert!(labels.contains(&"Set.isDisjoint"));
    }

    #[test]
    fn test_fetch_map_set_stdlib_completions_omit_runtime_only_surface_entries() {
        let labels: Vec<_> = fetch_map_set::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(!labels.contains(&"Map.keys"));
        assert!(!labels.contains(&"Map.toList"));
        assert!(!labels.contains(&"Map.update"));
        assert!(!labels.contains(&"Map.map"));
        assert!(!labels.contains(&"Map.mapWithKey"));
        assert!(!labels.contains(&"Map.filter"));
        assert!(!labels.contains(&"Map.filterWithKey"));
        assert!(!labels.contains(&"Map.fold"));
        assert!(!labels.contains(&"Map.foldWithKey"));
        assert!(!labels.contains(&"Set.toList"));
        assert!(!labels.contains(&"Set.map"));
        assert!(!labels.contains(&"Set.filter"));
        assert!(!labels.contains(&"Set.fold"));
        assert!(!labels.contains(&"Set.partition"));
    }

    #[test]
    fn test_math_stdlib_completions_match_explicit_surface() {
        let labels: Vec<_> = math::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"math.pi"));
        assert!(labels.contains(&"math.e"));
        assert!(labels.contains(&"math.inf"));
        assert!(labels.contains(&"math.nan"));
        assert!(labels.contains(&"math.toInt"));
        assert!(labels.contains(&"math.toFloat"));
        assert!(labels.contains(&"math.isNan"));
        assert!(labels.contains(&"math.isInf"));
        assert!(labels.contains(&"math.floor"));
        assert!(labels.contains(&"math.ceil"));
        assert!(labels.contains(&"math.round"));
        assert!(labels.contains(&"math.sqrt"));
        assert!(labels.contains(&"math.log"));
        assert!(labels.contains(&"math.log10"));
        assert!(labels.contains(&"math.exp"));
        assert!(labels.contains(&"math.sin"));
        assert!(labels.contains(&"math.cos"));
        assert!(labels.contains(&"math.tan"));
        assert!(!labels.contains(&"math.abs"));
        assert!(!labels.contains(&"math.pow"));
    }

    #[test]
    fn test_path_stdlib_completions_match_real_surface() {
        let labels: Vec<_> = path::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(labels.contains(&"path.fromString"));
        assert!(labels.contains(&"path.joinPath"));
        assert!(labels.contains(&"path.filenamePath"));
        assert!(labels.contains(&"path.isAbsolutePath"));
        assert!(labels.contains(&"path.filename"));
        assert!(labels.contains(&"path.is_absolute"));
    }

    #[test]
    fn test_path_stdlib_completions_omit_stale_surface_entries() {
        let labels: Vec<_> = path::specs()
            .into_iter()
            .map(|(label, _, _, _)| label)
            .collect();

        assert!(!labels.contains(&"path.fileName"));
        assert!(!labels.contains(&"path.isAbsolute"));
    }
}
