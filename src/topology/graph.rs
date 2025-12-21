use std::collections::{BTreeMap, BTreeSet};

use crate::{
    gameplay::primitives::build::{AggregateOccupancy, Buildable, OccupancyGetter, Occupancy, Road},
    topology::{Intersection, Path, collision::CollisionChecker},
};

// neighbors v -> {e | v \in e}
// (better than v -> {v}, cause edge's invariant enforces correctness of a graph)

/// Not oriented graph
#[derive(Debug, Default)]
pub struct RoadGraph {
    edges: BTreeSet<Path>,
    out: BTreeMap<Intersection, BTreeSet<Path>>,
}

impl RoadGraph {
    pub fn iter(&self) -> impl Iterator<Item = Road> {
        self.edges.iter().map(|p| Road { pos: *p })
    }

    /// add an edge, no questions asked
    /// ---
    /// for inside use only basically
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

    /// add new road connected to existing
    pub fn extend(
        &mut self,
        edge: Path,
        full_occupancy: &AggregateOccupancy,
        this_occupancy: &AggregateOccupancy,
    ) -> Result<(), EdgeInsertationError> {
        let (v1, v2) = edge.intersections();
        let checker = CollisionChecker {
            roads: &self.edges,
            other_occupancy: full_occupancy,
            this_occupancy,
        };

        match checker.can_place(&Road { pos: edge }) {
            true => Ok(self.add_edge(edge)),
            false => Err(EdgeInsertationError),
        }
    }

    /// returns all possible extends for a road
    /// * `occupied` - all vertices occupied with building
    pub fn possible_road_placements(
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
        occupied: &Occupancy,
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

#[derive(Debug)]
pub struct EdgeInsertationError;
