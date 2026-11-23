use crate::*;
use horizon_core::nbt::clone_tag;

/// A component tree, retaining translation keys, styles and ordered children.
#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    /// The component's contents before its children.
    pub content: TextContent,
    /// Fields not set here inherit from the enclosing component.
    pub style: TextStyle,
    /// Components appended in order.
    pub children: Vec<Text>,
    /// Unrecognised component metadata survives an engine round trip, as a compound.
    pub metadata: Tag,
}

/// What supplies the characters or inline object of a component.
#[derive(Clone, Debug, PartialEq)]
pub enum TextContent {
    /// Literal characters, without interpreting legacy formatting codes.
    Literal(String),
    /// A translation remains a key and components, not a localized string.
    Translatable {
        /// Translation identifier.
        key: String,
        /// Template used when the key is not present in the language.
        fallback: Option<String>,
        /// Ordered `%s` arguments, retaining their own styles.
        arguments: Vec<Text>,
    },
    /// A key binding's localized display name.
    Keybind(String),
    /// A selector resolved by the game's component context.
    Selector {
        /// Selector expression.
        pattern: String,
        /// Component between the selected names.
        separator: Option<Box<Text>>,
    },
    /// A scoreboard value resolved by the game's component context.
    Score {
        /// Score holder or selector.
        name: String,
        /// Scoreboard objective.
        objective: String,
    },
    /// NBT resolved by the game's component context.
    Nbt {
        /// NBT path expression.
        path: String,
        /// Whether the selected strings contain components.
        interpret: bool,
        /// Where to obtain the NBT.
        source: TextNbtSource,
        /// Component between selected values.
        separator: Option<Box<Text>>,
    },
    /// A non-character inline object.
    Object(TextObject),
}

/// The source of an NBT component, not the NBT itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextNbtSource {
    /// Block position expression.
    Block(String),
    /// Entity selector expression.
    Entity(String),
    /// Command storage identifier.
    Storage(String),
}

/// An inline object embedded in text.
#[derive(Clone, Debug, PartialEq)]
pub enum TextObject {
    /// A player head, retaining the profile properties carried by the component.
    Player {
        /// Profile data, including signed skin properties when present.
        profile: TextProfile,
        /// Whether to draw the hat layer.
        hat: bool,
        /// Optional component for consumers unable to display the object; None uses the default.
        fallback: Option<Box<Text>>,
    },
    /// A sprite in a named atlas.
    Atlas {
        /// Atlas identifier.
        atlas: String,
        /// Sprite identifier.
        sprite: String,
        /// Optional component for consumers unable to display the object; None uses the default.
        fallback: Option<Box<Text>>,
    },
}

/// Optional style fields inherit; explicit `false` overrides an inherited decoration.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    /// Named colour or `#RRGGBB` component colour.
    pub color: Option<String>,
    /// Packed ARGB shadow colour; zero suppresses shadows.
    pub shadow_color: Option<u32>,
    /// Bold decoration.
    pub bold: Option<bool>,
    /// Italic decoration.
    pub italic: Option<bool>,
    /// Underline decoration.
    pub underlined: Option<bool>,
    /// Strikethrough decoration.
    pub strikethrough: Option<bool>,
    /// Obfuscated decoration.
    pub obfuscated: Option<bool>,
    /// Action invoked by clicking the component.
    pub click: Option<TextClick>,
    /// Information displayed on hover.
    pub hover: Option<TextHover>,
    /// Text inserted into the chat input on Shift-click.
    pub insertion: Option<String>,
    /// Font identifier.
    pub font: Option<String>,
    /// Unknown style fields, as a compound, without engine types.
    pub metadata: Tag,
}

/// A text component's click action.
///
/// File-opening actions are intentionally absent: mods can neither produce nor
/// round-trip `OpenFile`. Capture drops untrusted file-opening action metadata.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextClick {
    /// Open an HTTP or HTTPS URL.
    OpenUrl(String),
    /// Switch to the numbered page of a book.
    ChangePage(i32),
    /// Put a command in the chat input.
    SuggestCommand(String),
    /// Run a command.
    RunCommand(String),
    /// Copy text to the clipboard.
    CopyToClipboard(String),
    /// Show a dialog identifier or inline declaration.
    ShowDialog(Tag),
    /// A server-defined action with optional payload.
    Custom {
        /// Action identifier.
        id: String,
        /// Payload retained without interpretation.
        payload: Option<Tag>,
    },
}

/// A text component's hover action.
#[derive(Clone, Debug, PartialEq)]
pub enum TextHover {
    /// Show another styled component tree.
    Text(Box<Text>),
    /// Show an item with its component patch.
    Item {
        /// Item identifier, not a process-local registry slot.
        id: String,
        /// Display count.
        count: u32,
        /// Component patch carried by the component, if any.
        components: Option<Tag>,
    },
    /// Show an entity's identity and optional name.
    Entity {
        /// Entity type identifier.
        kind: String,
        /// UUID as the high and low halves.
        id: [u64; 2],
        /// Optional styled entity name.
        name: Option<Box<Text>>,
    },
}

impl Default for Text {
    fn default() -> Self {
        Self::literal("")
    }
}

impl Text {
    /// A component with the given contents, with no style overrides or children.
    pub fn new(content: TextContent) -> Self {
        Self {
            content,
            style: TextStyle::default(),
            children: Vec::new(),
            metadata: Tag::Compound(Default::default()),
        }
    }
    /// Check vanilla's component-depth bound before cloning any node payload.
    /// Counts every recursive component relation, including hover names.
    pub fn validate_depth(&self) -> Result<(), TextFormError> {
        let mut pending = vec![(self, 1)];
        while let Some((text, depth)) = pending.pop() {
            if depth > MAX_TEXT_DEPTH {
                return Err(TextFormError::TooDeep);
            }
            text.visit_children(|child, _| pending.push((child, depth + 1)));
        }
        Ok(())
    }
    /// A literal component, with no inherited style overrides or children.
    pub fn literal(value: impl Into<String>) -> Self {
        Self::new(TextContent::Literal(value.into()))
    }
    /// A translatable component. The key and arguments remain unresolved data.
    pub fn translate(key: impl Into<String>, arguments: Vec<Text>) -> Self {
        Self::new(TextContent::Translatable {
            key: key.into(),
            fallback: None,
            arguments,
        })
    }
    /// Append a child without flattening its style or translation.
    pub fn append(mut self, child: Text) -> Self {
        self.children.push(child);
        self
    }
    /// Set this component's style; children continue to inherit unset fields.
    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = style;
        self
    }
}

/// A player profile, including ordered signed properties and unknown data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextProfile {
    /// Player name in U+0021..U+007E, at most 16 characters (ExtraCodecs.PLAYER_NAME).
    pub name: Option<String>,
    /// UUID as high and low halves.
    pub id: Option<[u64; 2]>,
    /// Ordered signed records; unsigned map values retain their order within each name.
    pub properties: Vec<TextProfileProperty>,
    /// Unknown profile fields, as a compound.
    pub metadata: Tag,
}

/// One signed or unsigned profile property.
///
/// ExtraCodecs.PROPERTY_MAP accepts at most 16 distinct unsigned map names.
/// Signed or metadata-bearing records instead use its 16-entry list codec,
/// whose UTF-16 limits are 64 units for names, 32767 for values and 1024 for signatures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextProfileProperty {
    /// Property name.
    pub name: String,
    /// Property value.
    pub value: String,
    /// Optional signature.
    pub signature: Option<String>,
    /// Unknown property fields, as a compound.
    pub metadata: Tag,
}

impl Default for TextProfile {
    fn default() -> Self {
        Self {
            name: None,
            id: None,
            properties: Vec::new(),
            metadata: Tag::Compound(Default::default()),
        }
    }
}

impl Default for TextProfileProperty {
    fn default() -> Self {
        Self {
            name: String::new(),
            value: String::new(),
            signature: None,
            metadata: Tag::Compound(Default::default()),
        }
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            color: None,
            shadow_color: None,
            bold: None,
            italic: None,
            underlined: None,
            strikethrough: None,
            obfuscated: None,
            click: None,
            hover: None,
            insertion: None,
            font: None,
            metadata: Tag::Compound(Default::default()),
        }
    }
}

/// Flat representation of a component tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextForm {
    /// Nodes in parent-before-child order.
    pub nodes: Vec<TextNode>,
}

/// One typed shallow component and its position in the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextNode {
    /// Parent node index, or None for the root.
    pub parent: Option<u32>,
    /// Relation to the parent.
    pub relation: TextRelation,
    /// Contents without recursive component slots.
    pub content: TextNodeContent,
    /// Style without recursive component slots.
    pub style: TextNodeStyle,
    /// Unknown component fields, as a compound.
    pub metadata: Tag,
}

impl TextNode {
    /// A root row with no style overrides or unknown fields.
    pub fn new(content: TextNodeContent) -> Self {
        Self {
            parent: None,
            relation: TextRelation::Child,
            content,
            style: TextNodeStyle::default(),
            metadata: Tag::Compound(Default::default()),
        }
    }
}

/// Shallow content. Recursive values appear only in related rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextNodeContent {
    /// Literal characters.
    Literal(String),
    /// Translation key and optional literal fallback template.
    Translatable {
        /// Translation identifier.
        key: String,
        /// Literal fallback template.
        fallback: Option<String>,
    },
    /// Key binding identifier.
    Keybind(String),
    /// Selector expression.
    Selector {
        /// Selector expression.
        pattern: String,
    },
    /// Score holder and objective.
    Score {
        /// Score holder or selector.
        name: String,
        /// Scoreboard objective.
        objective: String,
    },
    /// NBT lookup.
    Nbt {
        /// NBT path expression.
        path: String,
        /// Whether selected strings contain components.
        interpret: bool,
        /// NBT lookup source.
        source: TextNbtSource,
    },
    /// Player head.
    Player {
        /// Typed player profile.
        profile: TextProfile,
        /// Whether to draw the hat layer.
        hat: bool,
    },
    /// Atlas sprite.
    Atlas {
        /// Atlas identifier.
        atlas: String,
        /// Sprite identifier.
        sprite: String,
    },
}

/// Shallow hover event. Text and entity-name values use related rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextNodeHover {
    /// Exactly one HoverText row is required.
    Text,
    /// Item identity and optional component patch.
    Item {
        /// Item identifier.
        id: String,
        /// Displayed item count.
        count: u32,
        /// Optional opaque component patch.
        components: Option<Tag>,
    },
    /// Entity identity; a HoverEntityName row is optional.
    Entity {
        /// Entity type identifier.
        kind: String,
        /// UUID as high and low halves.
        id: [u64; 2],
    },
}

/// The nonrecursive form of TextStyle, with the same inheritance semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextNodeStyle {
    /// Named colour or `#RRGGBB` component colour.
    pub color: Option<String>,
    /// Packed ARGB shadow colour; zero suppresses shadows.
    pub shadow_color: Option<u32>,
    /// Bold decoration.
    pub bold: Option<bool>,
    /// Italic decoration.
    pub italic: Option<bool>,
    /// Underline decoration.
    pub underlined: Option<bool>,
    /// Strikethrough decoration.
    pub strikethrough: Option<bool>,
    /// Obfuscated decoration.
    pub obfuscated: Option<bool>,
    /// Action invoked by clicking the component.
    pub click: Option<TextClick>,
    /// Information displayed on hover.
    pub hover: Option<TextNodeHover>,
    /// Text inserted into the chat input on Shift-click.
    pub insertion: Option<String>,
    /// Font identifier.
    pub font: Option<String>,
    /// Unknown style metadata remains owned data, without engine types.
    pub metadata: Tag,
}

/// Relation to an earlier row. Unknown discriminants are rejected by bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRelation {
    /// Appended component.
    Child,
    /// Translation argument.
    Argument,
    /// Selector or NBT separator.
    Separator,
    /// Inline object fallback.
    Fallback,
    /// Hover text.
    HoverText,
    /// Hover entity name.
    HoverEntityName,
}

/// Maximum component depth, counted in nodes.
pub const MAX_TEXT_DEPTH: usize = 512;

/// A refused Text crossing. No partially converted fields are written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextFormError {
    /// No backend is available.
    Unavailable,
    /// A component path or opaque NBT container path exceeds 512.
    TooDeep,
    /// Fields or parent/relation tables are inconsistent.
    Malformed,
    /// Tag→JSON emission cannot represent NaN or infinity.
    NonFinite,
    /// Tag→JSON emission cannot represent an unpaired UTF-16 surrogate.
    InvalidString,
    /// Vanilla JsonOps→NbtOps refuses null nested in metadata.
    NestedNull,
}

impl std::fmt::Display for TextFormError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "text backend unavailable",
            Self::TooDeep => "text or NBT nesting exceeds maximum depth",
            Self::Malformed => "malformed text fields or node table",
            Self::NonFinite => "JSON cannot represent a non-finite NBT number",
            Self::InvalidString => "JSON cannot represent an unpaired UTF-16 surrogate",
            Self::NestedNull => "JSON-to-NBT does not accept nested null",
        })
    }
}

impl std::error::Error for TextFormError {}

impl Text {
    /// Validate component paths, opaque NBT containers and typed profile fields
    /// before constructing owned conversion payloads. This does not impose JSON
    /// representation restrictions on NBT data; engine emission may refuse those.
    pub fn validate(&self) -> Result<(), TextFormError> {
        self.validate_depth()?;
        let mut pending = vec![self];
        while let Some(text) = pending.pop() {
            let profile = match &text.content {
                TextContent::Object(TextObject::Player { profile, .. }) => Some(profile),
                _ => None,
            };
            let item = match &text.style.hover {
                Some(TextHover::Item { components, .. }) => components.as_ref(),
                _ => None,
            };
            validate_payload(
                &text.metadata,
                &text.style.metadata,
                text.style.click.as_ref(),
                item,
                profile,
            )?;
            text.visit_children(|child, _| pending.push(child));
        }
        Ok(())
    }
    /// Visit recursive component slots in their relation order, without cloning.
    /// Opaque NBT data is not a component edge.
    pub fn visit_children<'a>(&'a self, mut visit: impl FnMut(&'a Text, TextRelation)) {
        for child in &self.children {
            visit(child, TextRelation::Child);
        }
        match &self.content {
            TextContent::Translatable { arguments, .. } => {
                for child in arguments {
                    visit(child, TextRelation::Argument);
                }
            }
            TextContent::Selector { separator, .. } | TextContent::Nbt { separator, .. } => {
                if let Some(child) = separator {
                    visit(child, TextRelation::Separator);
                }
            }
            TextContent::Object(
                TextObject::Player { fallback, .. } | TextObject::Atlas { fallback, .. },
            ) => {
                if let Some(child) = fallback {
                    visit(child, TextRelation::Fallback);
                }
            }
            _ => {}
        }
        match &self.style.hover {
            Some(TextHover::Text(child)) => visit(child, TextRelation::HoverText),
            Some(TextHover::Entity {
                name: Some(child), ..
            }) => visit(child, TextRelation::HoverEntityName),
            _ => {}
        }
    }
}

fn clone_profile(source: &TextProfile) -> TextProfile {
    TextProfile {
        name: source.name.clone(),
        id: source.id,
        properties: source
            .properties
            .iter()
            .map(|p| TextProfileProperty {
                name: p.name.clone(),
                value: p.value.clone(),
                signature: p.signature.clone(),
                metadata: clone_tag(&p.metadata),
            })
            .collect(),
        metadata: clone_tag(&source.metadata),
    }
}

fn clone_click(source: &TextClick) -> TextClick {
    match source {
        TextClick::OpenUrl(s) => TextClick::OpenUrl(s.clone()),
        TextClick::ChangePage(n) => TextClick::ChangePage(*n),
        TextClick::SuggestCommand(s) => TextClick::SuggestCommand(s.clone()),
        TextClick::RunCommand(s) => TextClick::RunCommand(s.clone()),
        TextClick::CopyToClipboard(s) => TextClick::CopyToClipboard(s.clone()),
        TextClick::ShowDialog(t) => TextClick::ShowDialog(clone_tag(t)),
        TextClick::Custom { id, payload } => TextClick::Custom {
            id: id.clone(),
            payload: payload.as_ref().map(clone_tag),
        },
    }
}

impl Default for TextNodeStyle {
    fn default() -> Self {
        Self {
            color: None,
            shadow_color: None,
            bold: None,
            italic: None,
            underlined: None,
            strikethrough: None,
            obfuscated: None,
            insertion: None,
            font: None,
            click: None,
            hover: None,
            metadata: Tag::Compound(Default::default()),
        }
    }
}

impl TextNodeStyle {
    fn from_style(source: &TextStyle) -> Self {
        let hover = source.hover.as_ref().map(|hover| match hover {
            TextHover::Text(_) => TextNodeHover::Text,
            TextHover::Item {
                id,
                count,
                components,
            } => TextNodeHover::Item {
                id: id.clone(),
                count: *count,
                components: components.as_ref().map(clone_tag),
            },
            TextHover::Entity { kind, id, .. } => TextNodeHover::Entity {
                kind: kind.clone(),
                id: *id,
            },
        });
        Self {
            color: source.color.clone(),
            shadow_color: source.shadow_color,
            bold: source.bold,
            italic: source.italic,
            underlined: source.underlined,
            strikethrough: source.strikethrough,
            obfuscated: source.obfuscated,
            insertion: source.insertion.clone(),
            font: source.font.clone(),
            click: source.click.as_ref().map(clone_click),
            hover,
            metadata: clone_tag(&source.metadata),
        }
    }
    fn into_style(self) -> TextStyle {
        let hover = self.hover.and_then(|hover| match hover {
            TextNodeHover::Text => None, // The required child row supplies the value.
            TextNodeHover::Item {
                id,
                count,
                components,
            } => Some(TextHover::Item {
                id,
                count,
                components,
            }),
            TextNodeHover::Entity { kind, id } => Some(TextHover::Entity {
                kind,
                id,
                name: None,
            }),
        });
        TextStyle {
            color: self.color,
            shadow_color: self.shadow_color,
            bold: self.bold,
            italic: self.italic,
            underlined: self.underlined,
            strikethrough: self.strikethrough,
            obfuscated: self.obfuscated,
            insertion: self.insertion,
            font: self.font,
            click: self.click,
            hover,
            metadata: self.metadata,
        }
    }
}

fn validate_payload(
    metadata: &Tag,
    style_metadata: &Tag,
    click: Option<&TextClick>,
    item: Option<&Tag>,
    profile: Option<&TextProfile>,
) -> Result<(), TextFormError> {
    let mut roots = vec![metadata, style_metadata];
    if let Some(profile) = profile {
        roots.push(&profile.metadata);
        roots.extend(profile.properties.iter().map(|p| &p.metadata));
        // ResolvableProfile.Partial.MAP_CODEC and ExtraCodecs.PLAYER_NAME.
        if profile
            .name
            .as_ref()
            .is_some_and(|n| n.len() > 16 || n.bytes().any(|b| b <= 32 || b >= 127))
        {
            return Err(TextFormError::Malformed);
        }
        // ExtraCodecs.PROPERTY_MAP accepts either up to16 map keys with
        // unbounded unsigned value lists, or up to16 signed property records.
        let unsigned = profile.properties.iter().all(|p| {
            p.signature.is_none() && matches!(&p.metadata, Tag::Compound(m) if m.is_empty())
        });
        if unsigned {
            let names: std::collections::BTreeSet<_> =
                profile.properties.iter().map(|p| &p.name).collect();
            if names.len() > 16 {
                return Err(TextFormError::Malformed);
            }
        } else if profile.properties.len() > 16
            || profile.properties.iter().any(|p| {
                p.name.encode_utf16().count() > 64
                    || p.value.encode_utf16().count() > 32767
                    || p.signature
                        .as_ref()
                        .is_some_and(|s| s.encode_utf16().count() > 1024)
            })
        {
            return Err(TextFormError::Malformed);
        }
    }
    if roots.iter().any(|t| !matches!(t, Tag::Compound(_))) {
        return Err(TextFormError::Malformed);
    }
    if let Some(TextClick::ShowDialog(tag)) = click {
        roots.push(tag);
    }
    if let Some(TextClick::Custom {
        payload: Some(tag), ..
    }) = click
    {
        roots.push(tag);
    }
    roots.extend(item);
    if roots
        .into_iter()
        .any(|tag| !horizon_core::nbt::within_depth(tag, MAX_TEXT_DEPTH))
    {
        return Err(TextFormError::TooDeep);
    }
    Ok(())
}

impl TryFrom<&Text> for TextForm {
    type Error = TextFormError;
    fn try_from(value: &Text) -> Result<Self, Self::Error> {
        value.validate()?;
        let mut nodes = Vec::new();
        let mut pending = vec![(value, None, TextRelation::Child)];
        while let Some((text, parent, relation)) = pending.pop() {
            let index = u32::try_from(nodes.len()).map_err(|_| TextFormError::Malformed)?;
            let content = match &text.content {
                TextContent::Literal(s) => TextNodeContent::Literal(s.clone()),
                TextContent::Translatable { key, fallback, .. } => TextNodeContent::Translatable {
                    key: key.clone(),
                    fallback: fallback.clone(),
                },
                TextContent::Keybind(s) => TextNodeContent::Keybind(s.clone()),
                TextContent::Selector { pattern, .. } => TextNodeContent::Selector {
                    pattern: pattern.clone(),
                },
                TextContent::Score { name, objective } => TextNodeContent::Score {
                    name: name.clone(),
                    objective: objective.clone(),
                },
                TextContent::Nbt {
                    path,
                    interpret,
                    source,
                    ..
                } => TextNodeContent::Nbt {
                    path: path.clone(),
                    interpret: *interpret,
                    source: source.clone(),
                },
                TextContent::Object(TextObject::Player { profile, hat, .. }) => {
                    TextNodeContent::Player {
                        profile: clone_profile(profile),
                        hat: *hat,
                    }
                }
                TextContent::Object(TextObject::Atlas { atlas, sprite, .. }) => {
                    TextNodeContent::Atlas {
                        atlas: atlas.clone(),
                        sprite: sprite.clone(),
                    }
                }
            };
            nodes.push(TextNode {
                parent,
                relation,
                content,
                style: TextNodeStyle::from_style(&text.style),
                metadata: clone_tag(&text.metadata),
            });
            let mut children = Vec::new();
            text.visit_children(|child, relation| children.push((child, Some(index), relation)));
            pending.extend(children.into_iter().rev());
        }
        Ok(Self { nodes })
    }
}

impl TextForm {
    fn validate(&self) -> Result<(), TextFormError> {
        let mut depths = Vec::with_capacity(self.nodes.len());
        let mut masks = vec![0u8; self.nodes.len()];
        for (index, node) in self.nodes.iter().enumerate() {
            let depth = match node.parent {
                None if index == 0 && node.relation == TextRelation::Child => 1usize,
                Some(parent) if (parent as usize) < index => depths[parent as usize] + 1,
                _ => return Err(TextFormError::Malformed),
            };
            if depth > MAX_TEXT_DEPTH {
                return Err(TextFormError::TooDeep);
            }
            depths.push(depth);
            if let Some(parent) = node.parent {
                let parent = parent as usize;
                let target = &self.nodes[parent];
                let bit = match node.relation {
                    TextRelation::Child => 0,
                    TextRelation::Argument
                        if matches!(target.content, TextNodeContent::Translatable { .. }) =>
                    {
                        0
                    }
                    TextRelation::Separator
                        if matches!(
                            target.content,
                            TextNodeContent::Selector { .. } | TextNodeContent::Nbt { .. }
                        ) =>
                    {
                        1
                    }
                    TextRelation::Fallback
                        if matches!(
                            target.content,
                            TextNodeContent::Player { .. } | TextNodeContent::Atlas { .. }
                        ) =>
                    {
                        2
                    }
                    TextRelation::HoverText
                        if matches!(target.style.hover, Some(TextNodeHover::Text)) =>
                    {
                        4
                    }
                    TextRelation::HoverEntityName
                        if matches!(target.style.hover, Some(TextNodeHover::Entity { .. })) =>
                    {
                        8
                    }
                    _ => return Err(TextFormError::Malformed),
                };
                if masks[parent] & bit != 0 {
                    return Err(TextFormError::Malformed);
                }
                masks[parent] |= bit;
            }
        }
        if self.nodes.is_empty() {
            return Err(TextFormError::Malformed);
        }
        for (index, node) in self.nodes.iter().enumerate() {
            if matches!(node.style.hover, Some(TextNodeHover::Text)) && masks[index] & 4 == 0 {
                return Err(TextFormError::Malformed);
            }
            let profile = match &node.content {
                TextNodeContent::Player { profile, .. } => Some(profile),
                _ => None,
            };
            let item = match &node.style.hover {
                Some(TextNodeHover::Item { components, .. }) => components.as_ref(),
                _ => None,
            };
            validate_payload(
                &node.metadata,
                &node.style.metadata,
                node.style.click.as_ref(),
                item,
                profile,
            )?;
        }
        Ok(())
    }
}

fn discard_form(form: TextForm) {
    let mut tags = Vec::new();
    for node in form.nodes {
        tags.extend([node.metadata, node.style.metadata]);
        if let Some(
            TextClick::ShowDialog(t)
            | TextClick::Custom {
                payload: Some(t), ..
            },
        ) = node.style.click
        {
            tags.push(t);
        }
        if let Some(TextNodeHover::Item {
            components: Some(t),
            ..
        }) = node.style.hover
        {
            tags.push(t);
        }
        if let TextNodeContent::Player { profile, .. } = node.content {
            tags.push(profile.metadata);
            tags.extend(profile.properties.into_iter().map(|p| p.metadata));
        }
    }
    for tag in tags {
        horizon_core::nbt::discard_tag(tag);
    }
}

impl TryFrom<TextForm> for Text {
    type Error = TextFormError;
    fn try_from(value: TextForm) -> Result<Self, Self::Error> {
        if let Err(error) = value.validate() {
            discard_form(value);
            return Err(error);
        }
        let mut nodes: Vec<_> = value
            .nodes
            .into_iter()
            .map(|node| {
                let content = match node.content {
                    TextNodeContent::Literal(s) => TextContent::Literal(s),
                    TextNodeContent::Translatable { key, fallback } => TextContent::Translatable {
                        key,
                        fallback,
                        arguments: Vec::new(),
                    },
                    TextNodeContent::Keybind(s) => TextContent::Keybind(s),
                    TextNodeContent::Selector { pattern } => TextContent::Selector {
                        pattern,
                        separator: None,
                    },
                    TextNodeContent::Score { name, objective } => {
                        TextContent::Score { name, objective }
                    }
                    TextNodeContent::Nbt {
                        path,
                        interpret,
                        source,
                    } => TextContent::Nbt {
                        path,
                        interpret,
                        source,
                        separator: None,
                    },
                    TextNodeContent::Player { profile, hat } => {
                        TextContent::Object(TextObject::Player {
                            profile,
                            hat,
                            fallback: None,
                        })
                    }
                    TextNodeContent::Atlas { atlas, sprite } => {
                        TextContent::Object(TextObject::Atlas {
                            atlas,
                            sprite,
                            fallback: None,
                        })
                    }
                };
                (
                    node.parent,
                    node.relation,
                    Some(Text {
                        content,
                        style: node.style.into_style(),
                        children: Vec::new(),
                        metadata: node.metadata,
                    }),
                )
            })
            .collect();
        for i in (1..nodes.len()).rev() {
            let parent = nodes[i].0.expect("validated parent") as usize;
            let relation = nodes[i].1;
            let mut child = nodes[i].2.take().expect("unattached child");
            child.children.reverse();
            if let TextContent::Translatable { arguments, .. } = &mut child.content {
                arguments.reverse();
            }
            let target = nodes[parent].2.as_mut().expect("parent follows child");
            match relation {
                TextRelation::Child => target.children.push(child),
                TextRelation::Argument => match &mut target.content {
                    TextContent::Translatable { arguments, .. } => arguments.push(child),
                    _ => unreachable!("validated argument"),
                },
                TextRelation::Separator => match &mut target.content {
                    TextContent::Selector { separator, .. }
                    | TextContent::Nbt { separator, .. } => *separator = Some(Box::new(child)),
                    _ => unreachable!("validated separator"),
                },
                TextRelation::Fallback => match &mut target.content {
                    TextContent::Object(
                        TextObject::Player { fallback, .. } | TextObject::Atlas { fallback, .. },
                    ) => *fallback = Some(Box::new(child)),
                    _ => unreachable!("validated fallback"),
                },
                TextRelation::HoverText => {
                    target.style.hover = Some(TextHover::Text(Box::new(child)))
                }
                TextRelation::HoverEntityName => match &mut target.style.hover {
                    Some(TextHover::Entity { name, .. }) => *name = Some(Box::new(child)),
                    _ => unreachable!("validated name"),
                },
            }
        }
        let mut root = nodes[0].2.take().expect("validated root");
        root.children.reverse();
        if let TextContent::Translatable { arguments, .. } = &mut root.content {
            arguments.reverse();
        }
        Ok(root)
    }
}
