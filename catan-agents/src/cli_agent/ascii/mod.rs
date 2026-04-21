pub mod buffer;
pub mod cursor;
pub mod field_render;

pub mod test {
    use super::field_render::*;
    use catan_core::{
        gameplay::{
            game::init::GameInitializationState,
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

    #[test]
    fn hex_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(Hex::new(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Robber);
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_hex_attr(Hex::new(1, 0), HexAttr::TileNum(6));
        field.draw_hex_attr(Hex::new(1, 0), HexAttr::Selector);
        field.render();
    }

    #[test]
    fn path_attributes() {
        let mut field = FieldRenderer::new();

        field.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        field.draw_hex_attr(Hex::new(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Robber);
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Resource(Resource::Sheep));

        field.draw_hex_attr(Hex::new(1, 0), HexAttr::TileNum(6));
        field.draw_hex_attr(Hex::new(1, 0), HexAttr::Resource(Resource::Wood));
        field.draw_hex_attr(Hex::new(1, 0), HexAttr::Selector);

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

        field.draw_hex_attr(Hex::new(0, 0), HexAttr::TileNum(12));
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Robber);
        field.draw_hex_attr(Hex::new(0, 0), HexAttr::Resource(Resource::Sheep));

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
        let mut renderer = FieldRenderer::new();
        let mut game = GameInitializationState::default();

        // render skeleton
        renderer.draw_field(ColorSpec::new().set_fg(Some(Color::Ansi256(233))).clone());

        // render tile info
        for (hex, tile) in game.field.arrangement.hex_enum_iter() {
            match tile {
                Tile::Resource { resource, number } => {
                    renderer.draw_hex_attr(hex, HexAttr::TileNum(number.into()));
                    renderer.draw_hex_attr(hex, HexAttr::Resource(resource));
                }
                Tile::Desert => {}
                _ => todo!(),
            }
        }

        renderer.draw_hex_attr(game.field.robber_pos, HexAttr::Robber);

        let player_colors = [Color::Blue, Color::White, Color::Red, Color::Ansi256(172)];
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
            p(h(1, 0), h(2, 0)),
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

        for (id, player) in game.builds.players().iter().enumerate() {
            for road in player.roads.iter() {
                renderer.draw_path_attr(
                    road.pos,
                    PathAttr::Road {
                        color: player_colors[id],
                    },
                );
            }

            for est in player.establishments.iter() {
                renderer.draw_intersection_attr(
                    est.pos,
                    IntersectionAttr::Establishment(est.stage, player_colors[id]),
                );
            }
        }

        renderer.render();
    }
}
