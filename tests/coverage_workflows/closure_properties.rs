//! Property cover for the pull-request closure.
//!
//! The named cases pin one chain and one fan-out. The closure's claim is
//! wider: over any graph of local calls, including branches, cycles and
//! self-calls, it returns exactly the workflows reachable from one that a
//! pull request starts. These properties generate small call graphs, write
//! each as real workflow YAML with a randomly chosen call spelling, and
//! compare the closure with an independent reachability computed on the
//! adjacency matrix.

use std::collections::BTreeSet;

use proptest::prelude::*;

use super::{pull_request_cases::parse, reader};

/// The largest graph generated. Small enough to keep a case cheap, large
/// enough for chains, branches and cycles several calls long.
const MAX_WORKFLOWS: usize = 6;

/// The spellings a local call may take, all of which must reach the file.
const CALL_PREFIXES: [&str; 3] = ["./", "$/", ""];

/// A generated call graph: which workflows a pull request starts, which
/// workflow calls which, and the spelling each call uses.
#[derive(Debug, Clone)]
struct CallGraph {
    /// Whether workflow `i` is triggered by a pull request.
    starts_on_pull_request: Vec<bool>,
    /// Whether workflow `i` calls workflow `j`, at `i * n + j`.
    calls: Vec<bool>,
    /// The prefix index each call uses, at `i * n + j`.
    spellings: Vec<usize>,
}

impl CallGraph {
    /// Returns the number of workflows in the graph.
    const fn len(&self) -> usize { self.starts_on_pull_request.len() }

    /// Returns whether workflow `i` calls workflow `j`.
    fn has_call(&self, i: usize, j: usize) -> bool {
        self.calls.get(i * self.len() + j).copied().unwrap_or(false)
    }

    /// Returns the prefix the call from `i` to `j` is spelled with.
    fn spelling(&self, i: usize, j: usize) -> &'static str {
        self.spellings
            .get(i * self.len() + j)
            .and_then(|&index| CALL_PREFIXES.get(index))
            .copied()
            .unwrap_or_default()
    }

    /// Returns whether a pull request starts workflow `i`.
    fn starts(&self, i: usize) -> bool {
        self.starts_on_pull_request.get(i).copied().unwrap_or(false)
    }

    /// Renders workflow `i` as YAML.
    fn workflow(&self, i: usize) -> String {
        let trigger = if self.starts(i) {
            "pull_request"
        } else {
            "workflow_call"
        };
        let jobs = (0..self.len())
            .filter(|&j| self.has_call(i, j))
            .map(|j| {
                let prefix = self.spelling(i, j);
                format!("  call_{j}:\n    uses: {prefix}.github/workflows/w{j}.yml\n")
            })
            .collect::<Vec<_>>()
            .concat();
        if jobs.is_empty() {
            format!("on: {trigger}\njobs: {{}}\n")
        } else {
            format!("on: {trigger}\njobs:\n{jobs}")
        }
    }

    /// Returns the workflows reachable from a pull-request trigger, computed
    /// by transitive closure over the adjacency matrix rather than by search.
    fn expected_closure(&self) -> BTreeSet<String> {
        let n = self.len();
        let mut reach: Vec<Vec<bool>> = (0..n)
            .map(|i| (0..n).map(|j| i == j || self.has_call(i, j)).collect())
            .collect();
        for k in 0..n {
            let through: Vec<bool> = reach
                .iter()
                .map(|row| row.get(k).copied().unwrap_or(false))
                .collect();
            let from_k = reach.get(k).cloned().unwrap_or_default();
            for (row, _) in reach
                .iter_mut()
                .zip(through)
                .filter(|(_, reaches_k)| *reaches_k)
            {
                row.iter_mut()
                    .zip(&from_k)
                    .for_each(|(cell, &k_reaches)| *cell |= k_reaches);
            }
        }
        (0..n)
            .filter(|&j| {
                reach
                    .iter()
                    .enumerate()
                    .any(|(i, row)| self.starts(i) && row.get(j).copied().unwrap_or(false))
            })
            .map(|j| format!("w{j}.yml"))
            .collect()
    }
}

/// Generates a call graph of one to [`MAX_WORKFLOWS`] workflows.
fn call_graph() -> impl Strategy<Value = CallGraph> {
    (1..=MAX_WORKFLOWS).prop_flat_map(|n| {
        (
            prop::collection::vec(any::<bool>(), n),
            prop::collection::vec(prop::bool::weighted(0.3), n * n),
            prop::collection::vec(0..CALL_PREFIXES.len(), n * n),
        )
            .prop_map(|(starts_on_pull_request, calls, spellings)| CallGraph {
                starts_on_pull_request,
                calls,
                spellings,
            })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Scenario: an arbitrary graph of local calls, in any spelling.
    ///
    /// Invariant: the closure is exactly the set of workflows reachable from
    /// one a pull request starts, cycles and self-calls included.
    #[test]
    fn the_closure_is_exactly_what_a_pull_request_can_reach(graph in call_graph()) {
        let mut all = reader::Workflows::new();
        for i in 0..graph.len() {
            let parsed = parse(&graph.workflow(i));
            prop_assert!(parsed.is_ok(), "workflow {i} did not parse: {:?}", parsed.err());
            if let Ok(workflow) = parsed {
                all.insert(format!("w{i}.yml"), workflow);
            }
        }
        prop_assert_eq!(reader::pull_request_closure(&all), graph.expected_closure());
    }
}
