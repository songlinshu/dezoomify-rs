use std::str::FromStr;
use std::sync::Arc;

use serde::{de, Deserialize, Deserializer};

use crate::Vec2d;

#[derive(Debug, Deserialize, PartialEq)]
pub struct KrpanoMetadata {
    pub image: Vec<KrpanoImage>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct KrpanoImage {
    pub tilesize: Option<u32>,
    #[serde(default = "default_base_index")]
    pub baseindex: u32,
    #[serde(rename = "$value")]
    pub level: Vec<KrpanoLevel>,
}

fn default_base_index() -> u32 { 1 }

pub struct LevelDesc {
    pub name: &'static str,
    pub size: Vec2d,
    pub tilesize: Option<Vec2d>,
    pub url: TemplateString<TemplateVariable>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct ShapeDesc {
    url: TemplateString<TemplateVariable>,
    multires: Option<String>,
}

#[derive(Deserialize, PartialEq, Debug)]
pub struct LevelAttributes {
    tiledimagewidth: u32,
    tiledimageheight: u32,
    #[serde(rename = "$value")]
    shape: Vec<KrpanoLevel>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum KrpanoLevel {
    Level(LevelAttributes),
    Cube(ShapeDesc),
    Cylinder(ShapeDesc),
    Flat(ShapeDesc),
    Left(ShapeDesc),
    Right(ShapeDesc),
    Front(ShapeDesc),
    Back(ShapeDesc),
    Up(ShapeDesc),
    Down(ShapeDesc),
}

impl KrpanoLevel {
    pub fn level_descriptions(self, size: Option<Vec2d>) -> Vec<Result<LevelDesc, &'static str>> {
        match self {
            Self::Level(LevelAttributes { tiledimagewidth, tiledimageheight, shape }) => {
                let size = Vec2d { x: tiledimagewidth, y: tiledimageheight };
                shape.into_iter().flat_map(|level| level.level_descriptions(Some(size))).collect()
            },
            Self::Cube(d) => shape_descriptions("Cube", d, size),
            Self::Cylinder(d) => shape_descriptions("Cylinder", d, size),
            Self::Flat(d) => shape_descriptions("Flat", d, size),
            Self::Left(d) => shape_descriptions("Left", d, size),
            Self::Right(d) => shape_descriptions("Right", d, size),
            Self::Front(d) => shape_descriptions("Front", d, size),
            Self::Back(d) => shape_descriptions("Back", d, size),
            Self::Up(d) => shape_descriptions("Up", d, size),
            Self::Down(d) => shape_descriptions("Down", d, size),
        }
    }
}

fn shape_descriptions(
    name: &'static str,
    desc: ShapeDesc,
    size: Option<Vec2d>,
) -> Vec<Result<LevelDesc, &'static str>> {
    let ShapeDesc { multires, url } = desc;
    if let Some(multires) = multires {
        parse_multires(&multires).into_iter().map(|result|
            result.map(|(size, tilesize)| LevelDesc {
                name,
                size,
                tilesize: Some(tilesize),
                url: url.clone(),
            })
        ).collect()
    } else if let Some(size) = size {
        let tilesize = None;
        vec![Ok(LevelDesc { name, size, tilesize, url })]
    } else {
        vec![Err("missing multires attribute")]
    }
}

/// Parse a multires string into a vector of (image size, tile_size)
fn parse_multires(s: &str) -> Vec<Result<(Vec2d, Vec2d), &'static str>> {
    let mut parts = s.split(',');
    let maybe_tilesize: Option<u32> = parts.next().and_then(|x| x.parse().ok());
    let tilesize_x = if let Some(t) = maybe_tilesize { t } else {
        return vec![Err("missing tilesize")]
    };
    parts.map(|dim_str| {
        let mut dims = dim_str.split('x');
        let x: u32 = dims
            .next().ok_or("missing width")?
            .parse().map_err(|_| "invalid width")?;
        let y: u32 = dims
            .next().and_then(|x| x.parse().ok())
            .unwrap_or(x);
        let tilesize = dims.next()
            .and_then(|x| x.parse().ok())
            .unwrap_or(tilesize_x);
        Ok((Vec2d { x, y }, Vec2d::square(tilesize)))
    }).collect()
}

#[derive(Debug, PartialEq, Clone)]
pub struct TemplateString<T>(pub Vec<TemplateStringPart<T>>);

impl<'de> Deserialize<'de> for TemplateString<TemplateVariable> {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error> where
        D: Deserializer<'de> {
        use de::Error;
        String::deserialize(deserializer)?.parse().map_err(Error::custom)
    }
}


impl FromStr for TemplateString<TemplateVariable> {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        use itertools::Itertools;
        let mut chars = input.chars();
        let mut parts = vec![];
        loop {
            let literal: String = chars.take_while_ref(|&c| c != '%').collect();
            parts.push(TemplateStringPart::Literal(Arc::new(literal)));
            if chars.next().is_none() { break; }
            let padding = chars.take_while_ref(|&c| c == '0').count() as u32;
            let variable = match chars.next() {
                Some('h') | Some('x') | Some('u') | Some('c') => TemplateVariable::X,
                Some('v') | Some('y') | Some('r') => TemplateVariable::Y,
                Some('s') => TemplateVariable::Side,
                Some(x) => return Err(format!("Unknown template variable '{}' in '{}'", x, input)),
                None => return Err(format!("Invalid templating syntax in '{}'", input))
            };
            parts.push(TemplateStringPart::Variable { padding, variable })
        }
        Ok(TemplateString(parts))
    }
}

impl TemplateString<TemplateVariable> {
    pub fn all_sides(self) -> impl Iterator<Item=(&'static str, TemplateString<XY>)> + 'static {
        let has_side = self.0.iter().any(|x| match x {
            TemplateStringPart::Variable { variable, .. } => *variable == TemplateVariable::Side,
            _ => false
        });
        let sides = if has_side { &["forward", "back", "left", "right", "up", "down"][..] } else { &[""] };
        sides.iter().map(move |&side| (side, TemplateString(
            self.0.iter().map(|part| part.with_side(side)).collect()
        )))
    }
}


#[derive(Debug, PartialEq, Clone)]
pub enum TemplateStringPart<T> {
    Literal(Arc<String>),
    Variable { padding: u32, variable: T },
}

impl TemplateStringPart<TemplateVariable> {
    fn with_side(&self, side: &'static str) -> TemplateStringPart<XY> {
        match self {
            TemplateStringPart::Literal(s) => TemplateStringPart::Literal(Arc::clone(s)),
            TemplateStringPart::Variable { padding, variable } => {
                let padding = *padding;
                match variable {
                    TemplateVariable::X => TemplateStringPart::Variable { padding, variable: XY::X },
                    TemplateVariable::Y => TemplateStringPart::Variable { padding, variable: XY::Y },
                    TemplateVariable::Side => TemplateStringPart::Literal(Arc::new(side[..1].to_string())),
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum TemplateVariable { X, Y, Side }

#[derive(Debug, PartialEq, Clone)]
pub enum XY { X, Y }

#[cfg(test)]
mod test {
    use crate::krpano::krpano_metadata::KrpanoLevel::Left;
    use crate::krpano::krpano_metadata::TemplateStringPart::{Literal, Variable};
    use crate::krpano::krpano_metadata::TemplateVariable::{X, Y};

    use super::*;

    fn str(s: &str) -> TemplateStringPart<TemplateVariable> { Literal(Arc::new(s.to_string())) }

    fn x(padding: u32) -> TemplateStringPart<TemplateVariable> { Variable { padding, variable: X } }

    fn y(padding: u32) -> TemplateStringPart<TemplateVariable> { Variable { padding, variable: Y } }

    #[test]
    fn parse_xml_cylinder() {
        let parsed: KrpanoMetadata = serde_xml_rs::from_str(r#"
        <krpano version="1.18"  bgcolor="0xFFFFFF">
        <include url="skin/flatpano_setup.xml" />
        <view devices="mobile" hlookat="0" vlookat="0" maxpixelzoom="0.7" limitview="fullrange" fov="1.8" fovmax="1.8" fovmin="0.02"/>
            <preview url="monomane.tiles/preview.jpg" />
            <image type="CYLINDER" hfov="1.00" vfov="1.208146" voffset="0.00" multires="true" tilesize="512" progressive="true">
                <level tiledimagewidth="31646" tiledimageheight="38234">
                    <cylinder url="monomane.tiles/l7/%v/l7_%v_%h.jpg" />
                </level>
            </image>
        </krpano>
        "#).unwrap();
        assert_eq!(parsed, KrpanoMetadata {
            image: vec![
                KrpanoImage {
                    baseindex: 1,
                    tilesize: Some(512),
                    level: vec![
                        KrpanoLevel::Level(LevelAttributes {
                            tiledimagewidth: 31646,
                            tiledimageheight: 38234,
                            shape: vec![KrpanoLevel::Cylinder(ShapeDesc {
                                url: TemplateString(vec![
                                    str("monomane.tiles/l7/"), y(0), str("/l7_"),
                                    y(0), str("_"), x(0), str(".jpg"),
                                ]),
                                multires: None,
                            })],
                        }),
                    ],
                }
            ]
        })
    }


    #[test]
    fn parse_xml_old_cube() {
        let parsed: KrpanoMetadata = serde_xml_rs::from_str(r#"<krpano showerrors="false" logkey="false">
        <image type="cube" multires="true" tilesize="512" baseindex="0" progressive="false" multiresthreshold="-0.3">
            <level download="view" decode="view" tiledimagewidth="3280" tiledimageheight="3280">
                <left  url="https://example.com/%000r/%0000c.jpg"/>
            </level>
        </image>
        </krpano>"#).unwrap();
        assert_eq!(parsed, KrpanoMetadata {
            image: vec![KrpanoImage {
                baseindex: 0,
                tilesize: Some(512),
                level: vec![KrpanoLevel::Level(LevelAttributes {
                    tiledimagewidth: 3280,
                    tiledimageheight: 3280,
                    shape: vec![
                        Left(ShapeDesc {
                            url: TemplateString(vec![
                                str("https://example.com/"), y(3), str("/"),
                                x(4), str(".jpg")]),
                            multires: None,
                        })],
                })],
            }]
        })
    }

    #[test]
    fn parse_xml_multires() {
        let parsed: KrpanoMetadata = serde_xml_rs::from_str(r#"
        <krpano>
        <image>
            <flat url="https://example.com/" multires="512,768x554,1664x1202,3200x2310,6400x4618,12800x9234"/>
        </image>
        </krpano>"#).unwrap();
        assert_eq!(parsed, KrpanoMetadata {
            image: vec![KrpanoImage {
                baseindex: 1,
                tilesize: None,
                level: vec![KrpanoLevel::Flat(ShapeDesc {
                    url: TemplateString(vec![str("https://example.com/"), ]),
                    multires: Some("512,768x554,1664x1202,3200x2310,6400x4618,12800x9234".to_string()),
                })],
            }]
        })
    }

    #[test]
    fn multires_parse() {
        assert_eq!(vec![
            Ok((Vec2d { x: 6, y: 7 }, Vec2d { x: 3, y: 3 })),
            Ok((Vec2d { x: 8, y: 8 }, Vec2d { x: 3, y: 3 })),
            Ok((Vec2d { x: 9, y: 1 }, Vec2d { x: 4, y: 4 })),
        ], parse_multires("3,6x7,8x8,9x1x4"))
    }
}