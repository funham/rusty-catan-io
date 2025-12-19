use std::collections::{BTreeMap, BTreeSet};

use crate::topology::{Intersection, Path};

// neighbors v -> {e | v \in e}
// (better than v -> {v}, cause edge's invariant enforces correctness of a graph)

/// Not oriented graph
#[derive(Debug, Default)]
pub struct RoadGraph {
    edges: BTreeSet<Path>,
    out: BTreeMap<Intersection, BTreeSet<Path>>,
}

#[derive(Debug)]
pub enum EdgeInsertationError {
    EdgeIsNotCoincidentialError,
    CantContinueBrokenPath,
}

pub struct IncedenceChecker<'a, 'b> {
    graph: &'a RoadGraph,
    occupied_by_paths: &'b BTreeSet<Intersection>,
    occupied_by_builds: &'b BTreeSet<Intersection>, // occupied_by_builds => occupied_by_paths
    claimed_by_builds: &'b BTreeSet<Intersection>,
}

impl<'a, 'b> IncedenceChecker<'a, 'b> {
    /* technical methods */
    fn claimed_unchecked_(&self, v: &Intersection) -> bool {
        self.graph.out.contains_key(v)
    }

    fn occupied_unchecked_(&self, v: &Intersection) -> bool {
        self.occupied_by_paths.contains(v)
    }

    fn occupied_by_builds_unchecked_(&self, v: &Intersection) -> bool {
        self.occupied_by_builds.contains(v)
    }

    fn claimed_by_builds_unchecked_(&self, v: &Intersection) -> bool {
        self.claimed_by_builds.contains(v)
    }

    /// A -> B === True
    fn check_correctness(&self, v: &Intersection) -> bool {
        !self.claimed_unchecked_(v) || self.occupied_unchecked_(v)
    }

    fn assert_invariant(&self, v: &Intersection) {
        if !self.check_correctness(v) {
            unreachable!("connected must always be a subset of occupied")
        }
    }

    fn check_correctness_builds(&self, v: &Intersection) -> bool {
        !self.claimed_by_builds_unchecked_(v) || self.occupied_by_builds_unchecked_(v)
    }

    fn assert_invariant_builds(&self, v: &Intersection) {
        if !self.check_correctness_builds(v) {
            unreachable!("connected must always be a subset of occupied")
        }
    }

    /* public interface */

    /// A - claimed (by roads)
    pub fn claimed(&self, v: &Intersection) -> bool {
        self.assert_invariant(v);
        self.claimed_unchecked_(v)
    }

    /// B - occupied
    pub fn occupied(&self, v: &Intersection) -> bool {
        self.assert_invariant(v);
        self.occupied_unchecked_(v)
    }

    /// (A ^ B) - occupied by others or [Infallible]
    pub fn occupied_by_others(&self, v: &Intersection) -> bool {
        self.claimed(v) ^ self.occupied(v)
    }

    // !B (=> !A) - free of all
    pub fn free(&self, v: &Intersection) -> bool {
        !self.occupied(v)
    }

    /// has anybody's building on it
    pub fn occupied_by_builds(&self, v: &Intersection) -> bool {
        self.assert_invariant_builds(v);
        self.occupied_by_builds_unchecked_(v)
    }

    /// has our building on it
    pub fn claimed_by_builds(&self, v: &Intersection) -> bool {
        self.assert_invariant_builds(v);
        self.claimed_by_builds_unchecked_(v)
    }

    pub fn occupied_by_others_builds(&self, v: &Intersection) -> bool {
        self.claimed_by_builds(v) ^ self.occupied_by_builds(v)
    }
}

impl RoadGraph {
    /// add an edge, no questions asked
    pub fn add_edge(&mut self, edge: Path) {
        let (v1, v2) = edge.intersections();
        let _ = match self.out.get_mut(&v1) {
            Some(edges) => edges.insert(edge),
            None => self.out.insert(v1, BTreeSet::from([edge])).is_none(),
        };
        let _ = match self.out.get_mut(&v2) {
            Some(edges) => edges.insert(edge),
            None => self.out.insert(v2, BTreeSet::from([edge])).is_none(),
        };
    }

    pub fn incedence_checker<'a, 'b>(
        &'a self,
        occupied_by_paths: &'b BTreeSet<Intersection>,
        occupied_by_builds: &'b BTreeSet<Intersection>,
        claimed_by_builds: &'b BTreeSet<Intersection>,
    ) -> IncedenceChecker<'a, 'b> {
        IncedenceChecker {
            graph: self,
            occupied_by_paths,
            occupied_by_builds,
            claimed_by_builds,
        }
    }

    pub fn roads_occupancy(&self) -> &BTreeSet<Path> {
        &self.edges
    }

    /// add new road connected to the graph
    pub fn extend(
        &mut self,
        edge: Path,
        occupied_by_paths: &BTreeSet<Intersection>,
        occupied_by_builds: &BTreeSet<Intersection>,
        claimed_by_builds: &BTreeSet<Intersection>,
    ) -> Result<(), EdgeInsertationError> {
        let (v1, v2) = edge.intersections();
        let checker =
            self.incedence_checker(occupied_by_paths, occupied_by_builds, claimed_by_builds);

        match (checker.claimed(&v1), checker.claimed(&v2)) {
            (true, _) if !checker.claimed_by_builds(&v1) => (),
            (_, true) if !checker.claimed_by_builds(&v2) => (),
            (false, false) => return Err(EdgeInsertationError::EdgeIsNotCoincidentialError),
            _ => return Err(EdgeInsertationError::CantContinueBrokenPath),
        }

        self.add_edge(edge);
        Ok(())
    }

    /// returns all possible extends for a road
    /// * `occupied` - all vertices occupied with building
    pub fn possible_extends(
        &self,
        occupied: &BTreeSet<Intersection>,
    ) -> impl IntoIterator<Item = Path> {
        let mut visited = BTreeSet::new();
        let mut result = BTreeSet::new();

        for vertex in self.out.keys() {
            if visited.contains(vertex) {
                continue;
            }
            self.connectable_vertices_dfs(*vertex, &mut visited, occupied, &mut result);
        }

        // |- incident => occupied
        // 1) (incident & occupied) | (!incident & !occupied) == !(incident ^ occupied)
        let is_good_vertex = |v| !(self.out.contains_key(&v) ^ occupied.contains(&v));
        let is_good_edge = |e: &&Path| e.intersections_iter().all(is_good_vertex);

        result
            .iter()
            .filter(is_good_edge)
            .cloned()
            .collect::<BTreeSet<_>>()
    }

    fn connectable_vertices_dfs(
        &self,
        vertex: Intersection,
        visited: &mut BTreeSet<Intersection>,
        occupied: &BTreeSet<Intersection>,
        result: &mut BTreeSet<Path>,
    ) {
        if visited.contains(&vertex) {
            return; // to be extra confident
        }

        visited.insert(vertex);

        let mut dead_end = true;
        for edge in self
            .out
            .get(&vertex)
            .expect("graph is corrupt: any vertex incidental to an edge must be contained in map")
        {
            let next = edge.opposite_or_panic(vertex);
            if visited.contains(&next) {
                continue;
            }

            dead_end = false;
            self.connectable_vertices_dfs(next, visited, occupied, result);
        }

        if dead_end {
            result.extend(vertex.paths());
        }
    }

    pub fn calculate_diameter(&self) -> usize {
        todo!()
    }
}
