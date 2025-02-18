use std::{
    fmt::Write,
    hash::{BuildHasherDefault, Hash},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Error;
use indexmap::IndexSet;
use rustc_hash::FxHasher;
use swc_core::{
    common::SourceMap,
    ecma::{
        ast::{EsVersion, Id, Module},
        atoms::JsWord,
        codegen::text_writer::JsWriter,
        parser::parse_file_as_module,
    },
    testing::{self, fixture, NormalizedOutput},
};

use super::{
    graph::{
        DepGraph, Dependency, InternedGraph, ItemId, ItemIdGroupKind, Mode, SplitModuleResult,
    },
    merge::Merger,
    Analyzer,
};

#[fixture("tests/tree-shaker/analyzer/**/input.js")]
fn test_fixture(input: PathBuf) {
    run(input);
}

fn run(input: PathBuf) {
    testing::run_test(false, |cm, _handler| {
        let fm = cm.load_file(&input).unwrap();

        let module = parse_file_as_module(
            &fm,
            Default::default(),
            EsVersion::latest(),
            None,
            &mut vec![],
        )
        .unwrap();

        let mut g = DepGraph::default();
        let (item_ids, mut items) = g.init(&module);

        let mut s = String::new();

        writeln!(s, "# Items\n").unwrap();
        writeln!(s, "Count: {}", item_ids.len()).unwrap();
        writeln!(s).unwrap();

        for (i, id) in item_ids.iter().enumerate() {
            let item = &items[id];

            let (index, kind) = match id {
                ItemId::Group(_) => continue,
                ItemId::Item { index, kind } => (*index, kind),
            };

            writeln!(s, "## Item {}: Stmt {}, `{:?}`", i + 1, index, kind).unwrap();
            writeln!(s, "\n```js\n{}\n```\n", print(&cm, &[&module.body[index]])).unwrap();

            if item.is_hoisted {
                writeln!(s, "- Hoisted").unwrap();
            }

            if item.side_effects {
                writeln!(s, "- Side effects").unwrap();
            }

            let f = |ids: &IndexSet<Id, BuildHasherDefault<FxHasher>>| {
                let mut s = String::new();
                for (i, id) in ids.iter().enumerate() {
                    if i == 0 {
                        write!(s, "`{}`", id.0).unwrap();
                    } else {
                        write!(s, ", `{}`", id.0).unwrap();
                    }
                }
                s
            };

            if !item.var_decls.is_empty() {
                writeln!(s, "- Declares: {}", f(&item.var_decls)).unwrap();
            }

            if !item.read_vars.is_empty() {
                writeln!(s, "- Reads: {}", f(&item.read_vars)).unwrap();
            }

            if !item.eventual_read_vars.is_empty() {
                writeln!(s, "- Reads (eventual): {}", f(&item.eventual_read_vars)).unwrap();
            }

            if !item.write_vars.is_empty() {
                writeln!(s, "- Write: {}", f(&item.write_vars)).unwrap();
            }

            if !item.eventual_write_vars.is_empty() {
                writeln!(s, "- Write (eventual): {}", f(&item.eventual_write_vars)).unwrap();
            }

            writeln!(s).unwrap();
        }

        let mut analyzer = Analyzer {
            g: &mut g,
            item_ids: &item_ids,
            items: &mut items,
            last_side_effects: Default::default(),
            vars: Default::default(),
        };

        let eventual_ids = analyzer.hoist_vars_and_bindings(&module);

        writeln!(s, "# Phase 1").unwrap();
        writeln!(s, "```mermaid\n{}```", render_graph(&item_ids, analyzer.g)).unwrap();

        analyzer.evaluate_immediate(&module, &eventual_ids);

        writeln!(s, "# Phase 2").unwrap();
        writeln!(s, "```mermaid\n{}```", render_graph(&item_ids, analyzer.g)).unwrap();

        analyzer.evaluate_eventual(&module);

        writeln!(s, "# Phase 3").unwrap();
        writeln!(s, "```mermaid\n{}```", render_graph(&item_ids, analyzer.g)).unwrap();

        analyzer.handle_exports(&module);

        writeln!(s, "# Phase 4").unwrap();
        writeln!(s, "```mermaid\n{}```", render_graph(&item_ids, analyzer.g)).unwrap();

        let mut condensed = analyzer.g.finalize(analyzer.items);

        writeln!(s, "# Final").unwrap();
        writeln!(
            s,
            "```mermaid\n{}```",
            render_mermaid(&mut condensed, &|buf: &Vec<ItemId>| format!(
                "Items: {:?}",
                buf
            ))
        )
        .unwrap();

        let uri_of_module: JsWord = "entry.js".into();

        {
            let mut g = analyzer.g.clone();
            g.handle_weak(Mode::Development);
            let SplitModuleResult { modules, .. } = g.split_module(&uri_of_module, analyzer.items);

            writeln!(s, "# Modules (dev)").unwrap();
            for (i, module) in modules.iter().enumerate() {
                writeln!(s, "## Part {}", i).unwrap();
                writeln!(s, "```js\n{}\n```", print(&cm, &[module])).unwrap();
            }

            let mut merger = Merger::new(SingleModuleLoader {
                modules: &modules,
                entry_module_uri: &uri_of_module,
            });
            let module = merger.merge_recursively(modules[0].clone()).unwrap();

            writeln!(s, "## Merged (module eval)").unwrap();
            writeln!(s, "```js\n{}\n```", print(&cm, &[&module])).unwrap();
        }

        {
            let mut g = analyzer.g.clone();
            g.handle_weak(Mode::Production);
            let SplitModuleResult { modules, .. } = g.split_module(&uri_of_module, analyzer.items);

            writeln!(s, "# Modules (prod)").unwrap();
            for (i, module) in modules.iter().enumerate() {
                writeln!(s, "## Part {}", i).unwrap();
                writeln!(s, "```js\n{}\n```", print(&cm, &[module])).unwrap();
            }

            let mut merger = Merger::new(SingleModuleLoader {
                modules: &modules,
                entry_module_uri: &uri_of_module,
            });
            let module = merger.merge_recursively(modules[0].clone()).unwrap();

            writeln!(s, "## Merged (module eval)").unwrap();
            writeln!(s, "```js\n{}\n```", print(&cm, &[&module])).unwrap();
        }

        NormalizedOutput::from(s)
            .compare_to_file(input.with_file_name("output.md"))
            .unwrap();

        Ok(())
    })
    .unwrap();
}

struct SingleModuleLoader<'a> {
    entry_module_uri: &'a str,
    modules: &'a [Module],
}

impl super::merge::Load for SingleModuleLoader<'_> {
    fn load(&mut self, uri: &str, chunk_id: u32) -> Result<Option<Module>, Error> {
        if self.entry_module_uri == uri {
            return Ok(Some(self.modules[chunk_id as usize].clone()));
        }

        Ok(None)
    }
}

fn print<N: swc_core::ecma::codegen::Node>(cm: &Arc<SourceMap>, nodes: &[&N]) -> String {
    let mut buf = vec![];

    {
        let mut emitter = swc_core::ecma::codegen::Emitter {
            cfg: Default::default(),
            cm: cm.clone(),
            comments: None,
            wr: Box::new(JsWriter::new(cm.clone(), "\n", &mut buf, None)),
        };

        for n in nodes {
            n.emit_with(&mut emitter).unwrap();
        }
    }

    String::from_utf8(buf).unwrap()
}

fn render_graph(item_ids: &[ItemId], g: &mut DepGraph) -> String {
    let mut mermaid = String::from("graph TD\n");

    for (_, id) in item_ids.iter().enumerate() {
        let i = g.g.node(id);

        writeln!(mermaid, "    Item{};", i + 1).unwrap();

        if let Some(item_id) = render_item_id(id) {
            writeln!(mermaid, "    Item{}[\"{}\"];", i + 1, item_id).unwrap();
        }
    }

    for (from, to, kind) in g.g.idx_graph.all_edges() {
        writeln!(
            mermaid,
            "    Item{} -{}-> Item{};",
            from + 1,
            match kind {
                Dependency::Strong => "",
                Dependency::Weak => ".",
            },
            to + 1,
        )
        .unwrap();
    }

    mermaid
}

fn render_mermaid<T>(g: &mut InternedGraph<T>, render: &dyn Fn(&T) -> String) -> String
where
    T: Clone + Eq + Hash,
{
    let mut mermaid = String::from("graph TD\n");
    let ix = g.graph_ix.clone();

    for item in &ix {
        let i = g.node(item);

        writeln!(
            mermaid,
            "    N{}[\"{}\"];",
            i,
            render(item).replace('"', "\\\"").replace([';', '\n'], "")
        )
        .unwrap();
    }

    for (from, to, kind) in g.idx_graph.all_edges() {
        writeln!(
            mermaid,
            "    N{} -{}-> N{};",
            from,
            match kind {
                Dependency::Strong => "",
                Dependency::Weak => ".",
            },
            to,
        )
        .unwrap();
    }

    mermaid
}

fn render_item_id(id: &ItemId) -> Option<String> {
    match id {
        ItemId::Group(ItemIdGroupKind::ModuleEvaluation) => Some("ModuleEvaluation".into()),
        ItemId::Group(ItemIdGroupKind::Export(id)) => Some(format!("export {}", id.0)),
        _ => None,
    }
}
