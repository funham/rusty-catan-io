pub mod buffer;
pub mod cursor;
pub mod field_render;

pub mod test {
    use super::field_render::*;
    use catan_core::{
        gameplay::{
            game::{init::GameInitializationState, state::Perspective},
            primitives::{
                Tile,
                build::{Build, Establishment, EstablishmentType, Road},
                resource::Resource,
            },
        },
        topology::{Hex, Intersection, Path},
    };
    use termcolor::{Color, ColorSpec};

    fn h(q: i32, r: i32) -> Hex {
        Hex::new(q, r)
    }

    fn p(h1: Hex, h2: Hex) -> Path {
        Path::try_from((h1, h2)).unwrap()
    }

    fn v(h1: Hex, h2: Hex, h3: Hex) -> Intersection {
        Intersection::try_from((h1, h2, h3)).unwrap()
    }

    fn build_perspective() -> Perspective {
        let mut game = GameInitializationState::default();

        let settlements = [
            // player #0
            v(h(0, 0), h(1, 0), h(1, -1)),
            v(h(0, 0), h(-1, 1), h(0, 1)),
            //player #1
            v(h(-1, -1), h(-1, 0), h(0, -1)),
            v(h(0, -2), h(1, -2), h(1, -3)),
            // player #2
            v(h(1, 0), h(2, 0), h(2, -1)),
            v(h(0, 1), h(0, 2), h(-1, 2)),
            //player #3
            v(h(-2, 1), h(-3, 2), h(-3, 1)),
            v(h(-2, 0), h(-3, 1), h(-3, 0)),
        ];
        let roads = [
            // player #0
            p(h(0, 0), h(1, 0)),
            p(h(0, 0), h(0, 1)),
            // player #1
            p(h(-1, -1), h(-1, 0)),
            p(h(0, -2), h(1, -2)),
            // player #2
            p(h(1, 0), h(2, -1)),
            p(h(0, 1), h(0, 2)),
            // player #3
            p(h(-2, 1), h(-3, 2)),
            p(h(-2, 0), h(-3, 1)),
        ];

        for (i, (est_pos, road_pos)) in settlements.iter().zip(roads.iter()).enumerate() {
            game.builds
                .try_init_place(
                    i / 2,
                    Road { pos: *road_pos },
                    Establishment {
                        pos: *est_pos,
                        stage: EstablishmentType::Settlement,
                    },
                )
                .expect(&format!(
                    "try_build failed on settlement {:?} and road {:?}; builds snapshot: {:?}",
                    est_pos, road_pos, game.builds
                ));
        }

        game.perspective(0)
    }

    #[test]
    fn hex_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(h(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(h(0, 0), HexAttr::Robber);
        field.draw_hex_attr(h(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_hex_attr(h(1, 0), HexAttr::TileNum(6));
        field.draw_hex_attr(h(1, 0), HexAttr::Selector);
        field.render();
    }

    #[test]
    fn path_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(h(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(h(0, 0), HexAttr::Robber);
        field.draw_hex_attr(h(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_hex_attr(h(1, 0), HexAttr::TileNum(6));
        field.draw_hex_attr(h(1, 0), HexAttr::Resource(Resource::Wood));
        field.draw_hex_attr(h(1, 0), HexAttr::Selector);

        field.draw_path_attr(
            p(h(0, 0), h(0, 1)),
            PathAttr::Road {
                color: Color::Green,
            },
        );

        field.draw_path_attr(
            p(h(0, 0), h(-1, 1)),
            PathAttr::Road {
                color: Color::Green,
            },
        );

        field.render();
    }

    #[test]
    fn intersection_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(h(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(h(0, 0), HexAttr::Robber);
        field.draw_hex_attr(h(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_path_attr(
            p(h(0, 0), h(-1, 1)),
            PathAttr::Road {
                color: Color::Green,
            },
        );

        for (i, v) in h(0, 0).vertices().enumerate() {
            field.draw_intersection_attr(v, IntersectionAttr::Selector);
        }

        for (i, v) in h(-2, 0).vertices().enumerate() {
            field.draw_intersection_attr(
                v,
                IntersectionAttr::Establishment(EstablishmentType::Settlement, Color::Green),
            );
        }

        for (i, v) in h(2, 0).vertices().enumerate() {
            field.draw_intersection_attr(
                v,
                IntersectionAttr::Establishment(EstablishmentType::City, Color::Blue),
            );
        }

        field.render();
    }

    #[test]
    fn field_example() {
        let perspective = build_perspective();
        let mut renderer = FieldRenderer::new();

        renderer.draw_skeleton(&perspective);
        renderer.draw_tile_info(&perspective);
        renderer.draw_index(&perspective);
        renderer.draw_robber(&perspective);
        renderer.draw_builds(&perspective);

        renderer.render();
    }
}
