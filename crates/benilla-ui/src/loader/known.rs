//! What the loader reads of a document. The reference's walk logs an unknown frame type and an
//! unsupported handler, and nothing else; here an element or an attribute the loader never reads
//! is reported too ([`super::LoadReport::unknown_elements`], [`super::LoadReport::unknown_attributes`]),
//! counted and not fatal, so a build whose files carry more than the loader knows says so.
//!
//! The tables list what the loader's own reads ask for, per tag; a read added to the loader adds
//! its name here, which [`tests`] holds against the loader's sources.

use crate::framexml::Element;

/// Attributes every frame kind reads (`apply_attrs`, `apply_anchors` and the framework's own).
const FRAME_ATTRS: &[&str] = &[
    "alpha",
    "clampedtoscreen",
    "enablekeyboard",
    "enablemouse",
    "framelevel",
    "framestrata",
    "hidden",
    "id",
    "inherits",
    "movable",
    "name",
    "parent",
    "resizable",
    "setallpoints",
    "toplevel",
    "virtual",
];

/// Children every frame kind reads.
const FRAME_CHILDREN: &[&str] = &[
    "anchors",
    "backdrop",
    "frames",
    "hitrectinsets",
    "layers",
    "resizebounds",
    "scripts",
    "scrollchild",
    "size",
    "titleregion",
];

/// Attributes the document layer reads of any element.
const GENERIC_ATTRS: &[&str] = &["name", "virtual", "inherits"];

/// Attributes the loader reads, by lowercase tag, beyond the frame and generic sets.
const ATTRS: &[(&str, &[&str])] = &[
    ("absdimension", &["x", "y"]),
    ("absinset", &["bottom", "left", "right", "top"]),
    ("absvalue", &["val"]),
    ("anchor", &["point", "relativepoint", "relativeto"]),
    ("backdrop", &["bgfile", "edgefile", "tile"]),
    ("barcolor", &["a", "b", "g", "r"]),
    ("bartexture", &["file"]),
    ("button", &["checked", "scale", "text"]),
    (
        "buttontext",
        &["justifyh", "justifyv", "setallpoints", "text"],
    ),
    ("checkbutton", &["checked", "scale", "text"]),
    (
        "checkedtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("color", &["a", "b", "g", "r"]),
    ("colorselect", &["drawlayer", "scale"]),
    (
        "colorvaluetexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    (
        "colorvaluethumbtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    (
        "colorwheeltexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    (
        "colorwheelthumbtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    (
        "disabledcheckedtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("disabledfont", &["font", "justifyh"]),
    (
        "disabledtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("dressupmodel", &["file", "fogfar", "fognear", "scale"]),
    (
        "editbox",
        &[
            "autofocus",
            "blinkspeed",
            "historylines",
            "ignorearrows",
            "letters",
            "multiline",
            "numeric",
            "password",
            "scale",
        ],
    ),
    ("font", &["font", "justifyh", "justifyv", "outline"]),
    (
        "fontstring",
        &[
            "alpha",
            "font",
            "hidden",
            "justifyh",
            "justifyv",
            "setallpoints",
            "spacing",
            "text",
        ],
    ),
    ("frame", &["scale"]),
    ("gametooltip", &["scale"]),
    ("highlightfont", &["font", "justifyh"]),
    (
        "highlighttexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("layer", &["level"]),
    ("lootbutton", &["checked", "scale", "text"]),
    (
        "messageframe",
        &[
            "displayduration",
            "fade",
            "fadeduration",
            "insertmode",
            "scale",
        ],
    ),
    (
        "minimap",
        &["minimaparrowmodel", "minimapplayermodel", "scale"],
    ),
    ("model", &["file", "fogfar", "fognear", "scale"]),
    ("normalfont", &["font", "justifyh"]),
    (
        "normaltexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("offset", &["x", "y"]),
    ("playermodel", &["file", "fogfar", "fognear", "scale"]),
    (
        "pushedtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("scrollframe", &["scale"]),
    (
        "scrollingmessageframe",
        &[
            "displayduration",
            "fade",
            "fadeduration",
            "maxlines",
            "scale",
        ],
    ),
    ("simplehtml", &["file", "font", "hyperlinkformat", "scale"]),
    ("size", &["x", "y"]),
    (
        "slider",
        &[
            "defaultvalue",
            "drawlayer",
            "maxvalue",
            "minvalue",
            "orientation",
            "scale",
            "valuestep",
        ],
    ),
    (
        "statusbar",
        &[
            "defaultvalue",
            "drawlayer",
            "maxvalue",
            "minvalue",
            "orientation",
            "scale",
        ],
    ),
    ("tabardmodel", &["file", "fogfar", "fognear", "scale"]),
    ("taxirouteframe", &["scale"]),
    ("texcoords", &["bottom", "left", "right", "top"]),
    (
        "texture",
        &[
            "alpha",
            "alphamode",
            "file",
            "hidden",
            "justifyh",
            "justifyv",
            "setallpoints",
        ],
    ),
    (
        "thumbtexture",
        &["alphamode", "file", "justifyh", "justifyv", "setallpoints"],
    ),
    ("titleregion", &["justifyh", "justifyv", "setallpoints"]),
    ("worldframe", &["scale"]),
];

/// Child elements the loader reads, by lowercase tag, beyond the frame set.
const CHILDREN: &[(&str, &[&str])] = &[
    ("anchor", &["offset"]),
    ("anchors", &["anchor"]),
    (
        "backdrop",
        &[
            "backgroundinsets",
            "bordercolor",
            "color",
            "edgesize",
            "tilesize",
        ],
    ),
    ("backgroundinsets", &["absinset"]),
    ("bartexture", &["color"]),
    (
        "button",
        &[
            "buttontext",
            "checkedtexture",
            "disabledcheckedtexture",
            "disabledfont",
            "disabledtext",
            "disabledtexture",
            "highlightfont",
            "highlighttext",
            "highlighttexture",
            "normalfont",
            "normaltext",
            "normaltexture",
            "pushedtexture",
        ],
    ),
    ("buttontext", &["anchors", "size"]),
    (
        "checkbutton",
        &[
            "buttontext",
            "checkedtexture",
            "disabledcheckedtexture",
            "disabledfont",
            "disabledtext",
            "disabledtexture",
            "highlightfont",
            "highlighttext",
            "highlighttexture",
            "normalfont",
            "normaltext",
            "normaltexture",
            "pushedtexture",
        ],
    ),
    ("checkedtexture", &["anchors", "color", "size", "texcoords"]),
    (
        "colorselect",
        &[
            "colorvaluetexture",
            "colorvaluethumbtexture",
            "colorwheeltexture",
            "colorwheelthumbtexture",
        ],
    ),
    (
        "colorvaluetexture",
        &["anchors", "color", "size", "texcoords"],
    ),
    ("colorvaluethumbtexture", &["anchors", "size", "texcoords"]),
    (
        "colorwheeltexture",
        &["anchors", "color", "size", "texcoords"],
    ),
    ("colorwheelthumbtexture", &["anchors", "size", "texcoords"]),
    (
        "disabledcheckedtexture",
        &["anchors", "color", "size", "texcoords"],
    ),
    (
        "disabledtexture",
        &["anchors", "color", "size", "texcoords"],
    ),
    ("dressupmodel", &["fogcolor"]),
    ("edgesize", &["absvalue"]),
    ("editbox", &["fontstring", "textinsets"]),
    ("font", &["color", "fontheight", "shadow"]),
    ("fontheight", &["absvalue"]),
    ("fontstring", &["anchors", "color", "shadow", "size"]),
    (
        "highlighttexture",
        &["anchors", "color", "size", "texcoords"],
    ),
    ("hitrectinsets", &["absinset"]),
    ("layer", &["fontstring", "texture"]),
    ("layers", &["layer"]),
    (
        "lootbutton",
        &[
            "buttontext",
            "checkedtexture",
            "disabledcheckedtexture",
            "disabledfont",
            "disabledtext",
            "disabledtexture",
            "highlightfont",
            "highlighttext",
            "highlighttexture",
            "normalfont",
            "normaltext",
            "normaltexture",
            "pushedtexture",
        ],
    ),
    ("maxresize", &["absdimension"]),
    ("messageframe", &["fontstring"]),
    ("minresize", &["absdimension"]),
    ("model", &["fogcolor"]),
    ("normaltexture", &["anchors", "color", "size", "texcoords"]),
    ("offset", &["absdimension"]),
    ("playermodel", &["fogcolor"]),
    ("pushedtexture", &["anchors", "color", "size", "texcoords"]),
    ("resizebounds", &["maxresize", "minresize"]),
    ("scrollingmessageframe", &["fontstring"]),
    ("shadow", &["color", "offset"]),
    (
        "simplehtml",
        &[
            "fontstring",
            "fontstringheader1",
            "fontstringheader2",
            "fontstringheader3",
        ],
    ),
    ("size", &["absdimension"]),
    ("slider", &["thumbtexture"]),
    ("statusbar", &["barcolor", "bartexture"]),
    ("tabardmodel", &["fogcolor"]),
    (
        "texture",
        &["anchors", "color", "gradient", "size", "texcoords"],
    ),
    ("thumbtexture", &["anchors", "size", "texcoords"]),
    ("tilesize", &["absvalue"]),
    ("titleregion", &["anchors", "size"]),
];

/// What only the 5.1 dialect's loader reads (2.4.3's frame attributes, `<Attributes>` and `<Cooldown>`):
/// on 1.12.1 these are as unknown as any other.
const FRAME_ATTRS_243: &[&str] = &["protected"];
const FRAME_CHILDREN_243: &[&str] = &["attributes"];
const ATTRS_243: &[(&str, &[&str])] = &[
    ("attribute", &["name", "type", "value"]),
    ("cooldown", &["drawedge", "reverse"]),
];
const CHILDREN_243: &[(&str, &[&str])] = &[("attributes", &["attribute"])];

fn listed(table: &[(&str, &[&str])], tag: &str, name: &str) -> bool {
    table
        .iter()
        .any(|(t, names)| *t == tag && names.contains(&name))
}

fn attr_known(tag: &str, frame: bool, tbc: bool, attr: &str) -> bool {
    GENERIC_ATTRS.contains(&attr)
        || (frame && FRAME_ATTRS.contains(&attr))
        || listed(ATTRS, tag, attr)
        || (tbc && ((frame && FRAME_ATTRS_243.contains(&attr)) || listed(ATTRS_243, tag, attr)))
}

fn child_known(tag: &str, frame: bool, tbc: bool, child: &str) -> bool {
    (frame && FRAME_CHILDREN.contains(&child))
        || listed(CHILDREN, tag, child)
        || (tbc
            && ((frame && FRAME_CHILDREN_243.contains(&child)) || listed(CHILDREN_243, tag, child)))
}

/// What a document's elements hold that the loader never reads, each pair once.
#[derive(Default)]
pub(super) struct Audit {
    /// Whether the loader is the 5.1 dialect's, which reads the [`FRAME_ATTRS_243`] family too.
    pub(super) tbc: bool,
    pub(super) elements: Vec<String>,
    pub(super) attributes: Vec<String>,
}

impl Audit {
    fn note(list: &mut Vec<String>, what: String) {
        if !list.contains(&what) {
            list.push(what);
        }
    }

    /// One top-level element: a frame kind, a region template or a font. A tag that is none of
    /// those is an unknown frame type, which the load reports and skips.
    pub(super) fn top_level(&mut self, el: &Element, is_frame: &dyn Fn(&str) -> bool) {
        let tag = el.tag.to_ascii_lowercase();
        if is_frame(&el.tag) || ["texture", "fontstring", "font"].contains(&tag.as_str()) {
            self.walk(el, is_frame);
        }
    }

    fn walk(&mut self, el: &Element, is_frame: &dyn Fn(&str) -> bool) {
        let tag = el.tag.to_ascii_lowercase();
        let frame = is_frame(&el.tag);
        for (name, _) in el.attrs() {
            if !attr_known(&tag, frame, self.tbc, &name.to_ascii_lowercase()) {
                Self::note(&mut self.attributes, format!("<{} {}>", el.tag, name));
            }
        }
        for child in &el.children {
            let ctag = child.tag.to_ascii_lowercase();
            match tag.as_str() {
                // A frame list: each child is a frame kind, or an unknown frame type.
                "frames" | "scrollchild" => {
                    if is_frame(&child.tag) {
                        self.walk(child, is_frame);
                    }
                }
                // A handler: its body is the code, and the loader reads no attribute of it.
                "scripts" => {
                    for (name, _) in child.attrs() {
                        Self::note(&mut self.attributes, format!("<{} {}>", child.tag, name));
                    }
                }
                _ if child_known(&tag, frame, self.tbc, &ctag) => self.walk(child, is_frame),
                _ => Self::note(&mut self.elements, format!("<{}><{}>", el.tag, child.tag)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framexml;

    fn audit(xml: &str) -> Audit {
        let doc = framexml::parse(xml).unwrap();
        let mut a = Audit::default();
        for item in &doc.items {
            if let framexml::TopLevel::Template(el)
            | framexml::TopLevel::Instance(el)
            | framexml::TopLevel::Font(el) = item
            {
                a.top_level(el, &|t| crate::script::frame_kind_from_tag(t).is_some());
            }
        }
        a
    }

    #[test]
    fn what_the_loader_reads_passes_and_the_rest_is_named_once_per_pair() {
        let a = audit(
            r#"<Ui>
                <Button name="B" frameLevel="3" mystery="1">
                    <Size><AbsDimension x="1" y="2"/></Size>
                    <Gizmo/><Gizmo/>
                    <Layers><Layer level="ARTWORK"><FontString name="F" bogus="x"/></Layer></Layers>
                    <Scripts><OnLoad function="Foo"/></Scripts>
                    <Frames><Mystery nothing="1"/><Frame name="Kid" odd="1"/></Frames>
                </Button>
            </Ui>"#,
        );
        assert_eq!(a.elements, ["<Button><Gizmo>"]);
        assert_eq!(
            a.attributes,
            [
                "<Button mystery>",
                "<FontString bogus>",
                "<OnLoad function>",
                "<Frame odd>"
            ]
        );
    }

    #[test]
    fn a_nested_unknown_frame_type_is_not_audited_inside() {
        let a = audit(
            r#"<Ui><Frame name="F"><Frames><Nope zz="1"><Gizmo/></Nope></Frames></Frame></Ui>"#,
        );
        assert!(a.elements.is_empty() && a.attributes.is_empty());
    }

    /// Every name in the tables is a literal the loader's own sources read.
    #[test]
    fn every_listed_name_is_a_literal_in_the_loaders_sources() {
        let sources = [
            include_str!("mod.rs"),
            include_str!("backdrop.rs"),
            include_str!("geometry.rs"),
            include_str!("regions.rs"),
            include_str!("scripts.rs"),
            include_str!("widgets.rs"),
            include_str!("../framexml.rs"),
            include_str!("../script/simplehtml/mod.rs"),
        ]
        .join("\n")
        .to_ascii_lowercase();
        let literal = |n: &str| sources.contains(&format!("\"{n}\""));
        let mut missing = Vec::new();
        for n in FRAME_ATTRS
            .iter()
            .chain(FRAME_CHILDREN)
            .chain(GENERIC_ATTRS)
        {
            if !literal(n) {
                missing.push(*n);
            }
        }
        for n in FRAME_ATTRS_243.iter().chain(FRAME_CHILDREN_243) {
            if !literal(n) {
                missing.push(*n);
            }
        }
        for (_, names) in ATTRS
            .iter()
            .chain(CHILDREN)
            .chain(ATTRS_243)
            .chain(CHILDREN_243)
        {
            for n in *names {
                if !literal(n) {
                    missing.push(*n);
                }
            }
        }
        assert!(missing.is_empty(), "never read by the loader: {missing:?}");
    }
}
