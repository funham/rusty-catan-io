use std::{fs::File, io::BufReader};

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::{
    gameplay::{
        field::BoardArrangement,
        primitives::{PortKind, Tile, resource::Resource},
    },
    math::dice::TileNum,
};

#[derive(Debug, Serialize, Deserialize)]
struct LayoutDocumentJsonVal {
    schema: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    radius: u8,
    order: LayoutOrderJsonVal,
    tiles: SmallVec<[TileJsonVal; 37]>,
    #[serde(default)]
    ports: SmallVec<[PortJsonVal; 16]>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum LayoutOrderJsonVal {
    HexSpiralV1,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TileJsonVal {
    Desert,
    Resource { resource: Resource, number: u8 },
    Ocean,
}

#[derive(Debug, Serialize, Deserialize)]
struct PortJsonVal {
    position: crate::gameplay::field::PortPos,
    #[serde(flatten)]
    kind: PortKindJsonVal,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PortKindJsonVal {
    Universal,
    Special { resource: Resource },
}

impl Serialize for BoardArrangement {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let tile_info = self
            .iter()
            .map(|hex| match hex {
                Tile::Desert => TileJsonVal::Desert,
                Tile::Resource { resource, number } => TileJsonVal::Resource {
                    resource,
                    number: number.into(),
                },
                Tile::River { number } => TileJsonVal::Resource {
                    resource: Resource::Ore,
                    number: number.into(),
                },
            })
            .collect();
        let ports = self
            .ports()
            .iter()
            .map(|(position, kind)| PortJsonVal {
                position: *position,
                kind: (*kind).into(),
            })
            .collect();

        LayoutDocumentJsonVal {
            schema: LAYOUT_SCHEMA.to_owned(),
            id: None,
            radius: self.radius(),
            order: LayoutOrderJsonVal::HexSpiralV1,
            tiles: tile_info,
            ports,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BoardArrangement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = LayoutDocumentJsonVal::deserialize(deserializer)?;
        if raw.schema != LAYOUT_SCHEMA {
            return Err(serde::de::Error::custom("unsupported layout schema"));
        }
        let mut tile_info = Vec::with_capacity(raw.tiles.len());

        for hex in raw.tiles {
            match hex {
                TileJsonVal::Desert => tile_info.push(Tile::Desert),
                TileJsonVal::Resource { resource, number } => {
                    let number = TileNum::try_from(number)
                        .map_err(|_| serde::de::Error::custom("invalid tile number"))?;
                    tile_info.push(Tile::Resource { resource, number });
                }
                TileJsonVal::Ocean => {
                    return Err(serde::de::Error::custom(
                        "ocean tiles are reserved but not implemented",
                    ));
                }
            }
        }

        let ports = raw
            .ports
            .into_iter()
            .map(|port| (port.position, port.kind.into()))
            .collect();
        BoardArrangement::try_build(raw.radius, tile_info, ports)
            .map_err(|err| serde::de::Error::custom(format!("{err:?}")))
    }
}

const LAYOUT_SCHEMA: &str = "rusty-catan.layout.v1";

pub fn arrangement_from_json(path: &std::path::Path) -> Option<BoardArrangement> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).ok()
}

pub fn standard_4p_arrangement() -> BoardArrangement {
    serde_json::from_str(STANDARD_4P_LAYOUT).expect("embedded standard 4p layout should be valid")
}

impl From<PortKind> for PortKindJsonVal {
    fn from(value: PortKind) -> Self {
        match value {
            PortKind::Special(resource) => Self::Special { resource },
            PortKind::Universal => Self::Universal,
        }
    }
}

impl From<PortKindJsonVal> for PortKind {
    fn from(value: PortKindJsonVal) -> Self {
        match value {
            PortKindJsonVal::Special { resource } => Self::Special(resource),
            PortKindJsonVal::Universal => Self::Universal,
        }
    }
}

const STANDARD_4P_LAYOUT: &str = r#"{
  "schema": "rusty-catan.layout.v1",
  "id": "standard-4p",
  "radius": 2,
  "order": "hex_spiral_v1",
  "tiles": [
    { "kind": "desert" },
    { "kind": "resource", "resource": "brick", "number": 2 },
    { "kind": "resource", "resource": "wood", "number": 3 },
    { "kind": "resource", "resource": "sheep", "number": 4 },
    { "kind": "resource", "resource": "wheat", "number": 5 },
    { "kind": "resource", "resource": "ore", "number": 6 },
    { "kind": "resource", "resource": "wood", "number": 9 },
    { "kind": "resource", "resource": "sheep", "number": 10 },
    { "kind": "resource", "resource": "brick", "number": 11 },
    { "kind": "resource", "resource": "wheat", "number": 12 },
    { "kind": "resource", "resource": "ore", "number": 9 },
    { "kind": "resource", "resource": "wood", "number": 10 },
    { "kind": "resource", "resource": "sheep", "number": 8 },
    { "kind": "resource", "resource": "brick", "number": 8 },
    { "kind": "resource", "resource": "wheat", "number": 6 },
    { "kind": "resource", "resource": "ore", "number": 5 },
    { "kind": "resource", "resource": "wood", "number": 4 },
    { "kind": "resource", "resource": "sheep", "number": 3 },
    { "kind": "resource", "resource": "wheat", "number": 11 }
  ],
  "ports": [
    { "position": { "hex": { "q": 0, "r": 3 }, "orient": "QP" }, "kind": "universal" },
    { "position": { "hex": { "q": -2, "r": 3 }, "orient": "SP" }, "kind": "special", "resource": "wheat" },
    { "position": { "hex": { "q": -3, "r": 2 }, "orient": "SP" }, "kind": "special", "resource": "ore" },
    { "position": { "hex": { "q": -3, "r": 0 }, "orient": "RP" }, "kind": "universal" },
    { "position": { "hex": { "q": -1, "r": -2 }, "orient": "QN" }, "kind": "special", "resource": "sheep" },
    { "position": { "hex": { "q": 1, "r": -3 }, "orient": "QN" }, "kind": "universal" },
    { "position": { "hex": { "q": 3, "r": -3 }, "orient": "SN" }, "kind": "universal" },
    { "position": { "hex": { "q": 3, "r": -1 }, "orient": "RN" }, "kind": "special", "resource": "brick" },
    { "position": { "hex": { "q": 2, "r": 1 }, "orient": "RN" }, "kind": "special", "resource": "wood" }
  ]
}"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrangement_deserialization_rejects_seven_tile_number() {
        let json = r#"{
            "schema": "rusty-catan.layout.v1",
            "id": "invalid",
            "radius": 0,
            "order": "hex_spiral_v1",
            "tiles": [{ "kind": "resource", "resource": "brick", "number": 7 }],
            "ports": []
        }"#;

        let result = serde_json::from_str::<BoardArrangement>(json);

        assert!(result.is_err());
    }

    #[test]
    fn arrangement_serializes_with_layout_schema() {
        let arrangement = standard_4p_arrangement();

        let raw = serde_json::to_string(&arrangement).unwrap();

        assert!(raw.contains("\"schema\":\"rusty-catan.layout.v1\""));
        assert!(raw.contains("\"order\":\"hex_spiral_v1\""));
        assert!(!raw.contains("tile_info"));
        serde_json::from_str::<BoardArrangement>(&raw).unwrap();
    }
}
