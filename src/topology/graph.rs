use std::collections::{BTreeMap, BTreeSet};

use crate::{
    gameplay::primitives::build::Road,
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
        self.edges.iter().map(|p| Road { pos: p.clone() })
    }

    pub fn edges(&self) -> &BTreeSet<Path> {
        &self.edges
    }

    /// add an edge, no questions asked
    /// ---
    /// for inside use only basically
    pub fn add_edge(&mut self, edge: &Path) {
        let (v1, v2) = edge.intersections();
        let _ = match self.out.get_mut(&v1) {
            Some(edges) => edges.insert(edge.clone()),
            None => self
                .out
                .insert(v1, BTreeSet::from([edge.clone()]))
                .is_none(),
        };
        let _ = match self.out.get_mut(&v2) {
            Some(edges) => edges.insert(edge.clone()),
            None => self
                .out
                .insert(v2, BTreeSet::from([edge.clone()]))
                .is_none(),
        };
        self.edges.insert(edge.clone());
    }

    /// add new road connected to existing
    pub fn extend(
        &mut self,
        edge: &Path,
        checker: &CollisionChecker,
    ) -> Result<(), EdgeInsertationError> {
        match checker.can_place(&Road { pos: edge.clone() }) {
            true => Ok(self.add_edge(edge)),
            false => Err(EdgeInsertationError),
        }
    }

    /// returns all possible extends for a road
    pub fn possible_road_placements(
        &self,
        checker: &CollisionChecker,
    ) -> impl IntoIterator<Item = Path> {
        let mut visited = BTreeSet::new();
        let mut result = BTreeSet::new();

        for vertex in self.out.keys() {
            if visited.contains(vertex) {
                continue;
            }
            self.connectable_vertices_dfs(*vertex, &mut visited, checker, &mut result);
        }

        // |- incident => occupied
        // 1) (incident & occupied) | (!incident & !occupied) == !(incident ^ occupied)

        result
            .iter()
            .filter(|e| checker.can_place(&Road { pos: (*e).clone() }))
            .cloned()
            .collect::<BTreeSet<_>>()
    }

    fn connectable_vertices_dfs(
        &self,
        vertex: Intersection,
        visited: &mut BTreeSet<Intersection>,
        checker: &CollisionChecker,
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
            self.connectable_vertices_dfs(next, visited, checker, result);
        }

        if dead_end {
            result.extend(vertex.paths());
        }
    }

    pub fn possible_settlement_placements(
        &self,
        checker: CollisionChecker,
    ) -> impl IntoIterator<Item = Intersection> {
        checker
            .this_occupancy
            .roads_occupancy
            .occupancy
            .iter()
            .filter(|v| {
                let dead_zone = v.neighbors().into_iter().chain([**v]);

                checker
                    .full_occupancy()
                    .builds_occupancy
                    .is_disjoint(&dead_zone.collect())
            })
            .copied()
            .collect::<BTreeSet<_>>()
    }

    /// Find the longest sequence of non-repeating roads (edges can't repeat, vertices can).
    /// This is finding the longest trail in the graph.
    pub fn find_longest_trail_length(&self) -> usize {
        if self.edges.is_empty() {
            return 0;
        }

        let mut max_length = 0;
        let mut visited_components = BTreeSet::new();

        // Process each connected component separately
        for &start_vertex in self.out.keys() {
            if visited_components.contains(&start_vertex) {
                continue;
            }

            let component_vertices = self.collect_component(start_vertex, &mut visited_components);
            let component_longest = self.longest_trail_length_in_component(&component_vertices);
            max_length = max_length.max(component_longest);
        }

        max_length
    }

    /// Find longest trail (non-repeating edges) in a connected component
    fn longest_trail_length_in_component(&self, component: &[Intersection]) -> usize {
        self.longest_trail_dfs(component)
    }

    /// DFS to find longest trail (non-repeating edges)
    fn longest_trail_dfs(&self, component: &[Intersection]) -> usize {
        let mut max_length = 0;

        // Try starting from each vertex in the component
        for &start in component {
            let mut visited_edges = BTreeSet::new();
            self.dfs_calculate_longest_trail_length(start, &mut visited_edges, 0, &mut max_length);
        }

        max_length
    }

    /// DFS helper for finding longest trail
    fn dfs_calculate_longest_trail_length(
        &self,
        current: Intersection,
        visited_edges: &mut BTreeSet<Path>,
        current_length: usize,
        max_length: &mut usize,
    ) {
        // Update max length
        if current_length > *max_length {
            *max_length = current_length;
        }

        // Try all edges from current vertex
        if let Some(edges) = self.out.get(&current) {
            for edge in edges.iter() {
                if visited_edges.contains(&edge) {
                    continue;
                }

                // Mark edge as visited
                visited_edges.insert(edge.clone());

                // Move to the other endpoint
                let neighbor = edge.opposite_or_panic(current);

                // Recurse
                self.dfs_calculate_longest_trail_length(
                    neighbor,
                    visited_edges,
                    current_length + 1,
                    max_length,
                );

                // Backtrack
                visited_edges.remove(&edge);
            }
        }
    }

    /// Get the actual longest road (sequence of edges)
    pub fn find_longest_trail(&self) -> Vec<Path> {
        if self.edges.is_empty() {
            return Vec::new();
        }

        let mut best_path = Vec::new();
        let mut max_length = 0;

        // Try starting with each edge
        for start_edge in &self.edges {
            let mut visited_edges = BTreeSet::new();
            let mut current_path = Vec::new();

            self.dfs_find_longest_trail(
                start_edge.clone(),
                &mut visited_edges,
                &mut current_path,
                &mut best_path,
                &mut max_length,
            );
        }

        best_path
    }

    /// DFS to find and record the actual trail
    fn dfs_find_longest_trail(
        &self,
        current_edge: Path,
        visited_edges: &mut BTreeSet<Path>,
        current_path: &mut Vec<Path>,
        best_path: &mut Vec<Path>,
        max_length: &mut usize,
    ) {
        visited_edges.insert(current_edge.clone());
        current_path.push(current_edge.clone());

        // Check if this is the longest path so far
        if current_path.len() > *max_length {
            *max_length = current_path.len();
            *best_path = current_path.clone();
        }

        // Try to extend from both endpoints
        let (v1, v2) = current_edge.intersections();

        // Try extending from v1
        if let Some(edges) = self.out.get(&v1) {
            for next_edge in edges.iter().cloned() {
                if !visited_edges.contains(&next_edge) {
                    self.dfs_find_longest_trail(
                        next_edge,
                        visited_edges,
                        current_path,
                        best_path,
                        max_length,
                    );
                }
            }
        }

        // Try extending from v2
        if let Some(edges) = self.out.get(&v2) {
            for next_edge in edges.iter().cloned() {
                if !visited_edges.contains(&next_edge) {
                    self.dfs_find_longest_trail(
                        next_edge,
                        visited_edges,
                        current_path,
                        best_path,
                        max_length,
                    );
                }
            }
        }

        // Backtrack
        visited_edges.remove(&current_edge);
        current_path.pop();
    }

    /// Collect all vertices in the connected component containing `start`
    pub fn collect_component(
        &self,
        start: Intersection,
        visited: &mut BTreeSet<Intersection>,
    ) -> Vec<Intersection> {
        let mut component = Vec::new();
        let mut stack = Vec::new();

        stack.push(start);

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }

            visited.insert(current);
            component.push(current);

            // Add all unvisited neighbors to the stack
            if let Some(edges) = self.out.get(&current) {
                for edge in edges.iter().cloned() {
                    let neighbor = edge.opposite_or_panic(current);
                    if !visited.contains(&neighbor) {
                        stack.push(neighbor);
                    }
                }
            }
        }

        component
    }
}

#[derive(Debug)]
pub struct EdgeInsertationError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::hex::*;

    // Helper to create hexes
    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    // Helper to create intersections
    fn intersection(h1: Hex, h2: Hex, h3: Hex) -> Intersection {
        Intersection::try_from((h1, h2, h3)).unwrap()
    }

    // Helper to create paths
    fn path(h1: Hex, h2: Hex) -> Path {
        let s = format!("not adjacent! h1: {:?}, h2: {:?}", h1, h2);
        Path::try_from((h1, h2)).expect(&s)
    }

    #[test]
    fn test_empty_graph() {
        let graph = RoadGraph::default();
        assert_eq!(graph.find_longest_trail_length(), 0);
        assert_eq!(graph.find_longest_trail().len(), 0);
        assert!(graph.edges().is_empty());
        assert_eq!(graph.iter().count(), 0);
    }

    #[test]
    fn test_single_road() {
        let mut graph = RoadGraph::default();
        let p = path(h(0, 0), h(1, 0));
        graph.add_edge(&p);

        assert_eq!(graph.find_longest_trail_length(), 1);
        assert_eq!(graph.find_longest_trail(), vec![p]);
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.iter().count(), 1);
    }

    #[test]
    fn test_two_roads_in_line() {
        let mut graph = RoadGraph::default();

        let p1 = path(h(0, 0), h(1, 0));
        let p2 = path(h(1, -1), h(1, 0));

        graph.add_edge(&p1);
        graph.add_edge(&p2);

        assert_eq!(graph.find_longest_trail_length(), 2);
        let longest = graph.find_longest_trail();
        assert!(longest == vec![p1.clone(), p2.clone()] || longest == vec![p2, p1]);
    }

    #[test]
    fn test_disconnected_components() {
        let mut graph = RoadGraph::default();

        // Component 1: A--B (length 1)
        let p1 = path(h(0, 0), h(1, 0));

        // Component 2: C--D--E (length 2)
        let p2 = path(h(2, 0), h(3, 0));
        let p3 = path(h(3, 0), h(2, 1));

        graph.add_edge(&p1);
        graph.add_edge(&p2);
        graph.add_edge(&p3);

        // Longest component has 2 roads
        assert_eq!(graph.find_longest_trail_length(), 2);
    }

    #[test]
    fn test_star_shape() {
        let mut graph = RoadGraph::default();

        // Star: center B connected to A, C, D
        let center = h(0, 0);
        let p1 = path(center, h(1, 0)); // B-A
        let p2 = path(center, h(-1, 0)); // B-C
        let p3 = path(center, h(0, 1)); // B-D

        graph.add_edge(&p1);
        graph.add_edge(&p2);
        graph.add_edge(&p3);

        // Can only take 2 roads from a star (enter and exit from different arms)
        assert_eq!(graph.find_longest_trail_length(), 2);
    }

    #[test]
    fn test_cycle() {
        let mut graph = RoadGraph::default();

        for n in h(0, 0).neighbors() {
            graph.add_edge(&path(h(0, 0), n));
        }

        assert_eq!(graph.find_longest_trail_length(), 6);
    }

    #[test]
    fn test_path_with_branch() {
        let mut graph = RoadGraph::default();

        let p4 = path(h(1, -1), h(1, -2));

        for path in intersection(h(0, 0), h(0, -1), h(1, -1)).paths() {
            graph.add_edge(&path);
        }
        graph.add_edge(&p4);

        // Longest: A-B-C-D or D-C-B-E (length 3)
        assert_eq!(graph.find_longest_trail_length(), 3);
    }

    #[test]
    fn test_road_iteration() {
        let mut graph = RoadGraph::default();
        let roads = vec![
            path(h(0, 0), h(1, 0)),
            path(h(1, 0), h(2, 0)),
            path(h(2, 0), h(3, 0)),
        ];

        for road in roads.iter().cloned() {
            graph.add_edge(&road);
        }

        let iter_roads: Vec<Path> = graph.iter().map(|r| r.pos).collect();
        assert_eq!(iter_roads.len(), 3);
        for road in &roads {
            assert!(iter_roads.contains(road));
        }
    }

    #[test]
    fn test_collect_component() {
        let mut graph = RoadGraph::default();

        // Create three roads that form two disconnected components
        // Component 1: Single road (2 vertices)
        let p1 = path(h(0, 0), h(1, 0));

        // Component 2: Two connected roads forming a line of 3 vertices
        let p2 = path(h(2, 0), h(3, 0));
        let p3 = path(h(3, 0), h(2, 1));

        graph.add_edge(&p1);
        graph.add_edge(&p2);
        graph.add_edge(&p3);

        // Get any intersection from p1 to start component collection
        let (v1_start, _) = p1.intersections();

        // Get any intersection from p2 to start component collection
        let (v2_start, _) = p2.intersections();

        let mut visited = BTreeSet::new();

        // First component (single road): has 2 intersections
        let component1 = graph.collect_component(v1_start, &mut visited);
        assert_eq!(component1.len(), 2);

        // Second component (two roads in line): has 3 intersections
        let component2 = graph.collect_component(v2_start, &mut visited);
        assert_eq!(component2.len(), 3);

        // Verify components don't overlap
        let set1: BTreeSet<_> = component1.iter().collect();
        let set2: BTreeSet<_> = component2.iter().collect();
        assert!(set1.is_disjoint(&set2));
    }

    #[test]
    fn test_path_intersections() {
        let p = path(h(0, 0), h(1, 0));
        let (v1, v2) = p.intersections();

        // Verify both intersections contain the edge's hexes
        assert!(v1.as_set().contains(&h(0, 0)));
        assert!(v1.as_set().contains(&h(1, 0)));
        assert!(v2.as_set().contains(&h(0, 0)));
        assert!(v2.as_set().contains(&h(1, 0)));

        // Verify they're different
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_path_opposite() {
        let p = path(h(0, 0), h(1, 0));
        let (v1, v2) = p.intersections();

        assert_eq!(p.opposite(v1).unwrap(), v2);
        assert_eq!(p.opposite(v2).unwrap(), v1);

        // Try with wrong intersection
        let wrong_v = intersection(h(3, 0), h(2, 0), h(2, 1));
        assert!(p.opposite(wrong_v).is_err());
    }

    #[test]
    fn test_graph_structure_integrity() {
        let mut graph = RoadGraph::default();

        // Add several roads
        let roads = vec![
            path(h(0, 0), h(1, 0)),
            path(h(1, 0), h(1, -1)),
            path(h(1, -1), h(2, -1)),
        ];

        for road in roads.iter().cloned() {
            graph.add_edge(&road);
        }

        // Verify internal structure
        assert_eq!(graph.edges().len(), 3);

        // Check that out map is correctly populated
        let v = intersection(h(0, 0), h(1, 0), h(1, -1));
        assert!(graph.out.get(&v).unwrap().contains(&roads[0]));
        assert!(graph.out.get(&v).unwrap().contains(&roads[1]));
        assert!(!graph.out.get(&v).unwrap().contains(&roads[2]));
    }

    #[test]
    fn test_intersection_neighbors() {
        let v = intersection(h(0, 0), h(1, 0), h(0, 1));
        let neighbors = v.neighbors();

        // All neighbors should be different
        let unique_neighbors: BTreeSet<_> = neighbors.iter().collect();
        assert_eq!(unique_neighbors.len(), 3);
    }
}
