use std::{
    cell::{Ref, RefCell},
    collections::BTreeSet,
    rc::Rc,
};

use crate::{
    gameplay::primitives::build::{
        AggregateOccupancy, Buildable, OccupancyGetter, Road, Settlement,
    },
    topology::{HasPos, Intersection, Path, graph::RoadGraph},
};

pub struct CollisionChecker<'a, 'b> {
    pub other_occupancy: &'a AggregateOccupancy,
    pub this_occupancy: &'b AggregateOccupancy,
}

impl<'a, 'b> CollisionChecker<'a, 'b> {
    pub fn full_occupancy(&self) -> AggregateOccupancy {
        self.other_occupancy.union(self.this_occupancy)
    }

    pub fn can_place<T: Placeble>(&self, build: &T) -> bool {
        build.placeble(self)
    }

    pub fn contains<T: Containable>(&self, build: &T) -> bool {
        build.contained(self)
    }

    pub fn connected<T: OccupancyGetter>(&self, build: &T) -> bool {
        build
            .occupancy()
            .intersection(&self.this_occupancy.roads_occupancy.occupancy)
            .any(|_| true)
    }
}

pub trait Containable: Buildable {
    fn contained(&self, checker: &CollisionChecker) -> bool;
}

impl<T: Buildable + HasPos<Pos = Intersection>> Containable for T {
    fn contained(&self, checker: &CollisionChecker) -> bool {
        checker
            .this_occupancy
            .builds_occupancy
            .contains(&self.get_pos())
    }
}

impl Containable for Road {
    fn contained(&self, checker: &CollisionChecker) -> bool {
        checker
            .this_occupancy
            .roads_occupancy
            .paths
            .contains(&self.get_pos())
    }
}

pub trait Placeble: OccupancyGetter + HasPos {
    fn placeble(&self, checker: &CollisionChecker) -> bool;
}

impl<T: OccupancyGetter + HasPos<Pos = Intersection>> Placeble for T {
    fn placeble(&self, checker: &CollisionChecker) -> bool {
        let dead_zone = self.get_pos().neighbors().chain([self.get_pos()]).collect();
        checker.connected(self)
            && checker
                .full_occupancy()
                .builds_occupancy
                .is_disjoint(&dead_zone)
    }
}

impl Placeble for Road {
    fn placeble(&self, checker: &CollisionChecker) -> bool {
        let connected = |v| {
            checker
                .this_occupancy
                .roads_occupancy
                .occupancy
                .contains(&v)
        };
        let broken = |v| checker.other_occupancy.builds_occupancy.contains(&v);

        self.pos
            .intersections_iter()
            .all(|v| connected(v) && !broken(v))
    }
}
