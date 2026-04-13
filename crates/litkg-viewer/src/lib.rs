use anyhow::{anyhow, Result};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, ScrollArea, Sense, SidePanel, Stroke,
    TopBottomPanel, Vec2,
};
use litkg_neo4j::{load_export_bundle, Neo4jEdge, Neo4jExportBundle, Neo4jNode};
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
use petgraph::Direction;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f32::consts::TAU;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

pub fn run_bundle(bundle_root: impl AsRef<Path>) -> Result<()> {
    let bundle = load_export_bundle(bundle_root)?;
    run_export_bundle(bundle)
}

pub fn run_export_bundle(bundle: Neo4jExportBundle) -> Result<()> {
    let window_title = format!("litkg graph inspector - {}", bundle.root.display());
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 920.0]),
        ..Default::default()
    };

    eframe::run_native(
        window_title.as_str(),
        native_options,
        Box::new(move |_creation_context| Box::new(LitkgViewerApp::new(bundle))),
    )
    .map_err(|err| anyhow!("Failed to launch litkg graph inspector: {err}"))
}

#[derive(Clone, Debug)]
struct ViewerNode {
    id: String,
    kind: String,
    title: String,
    subtitle: String,
    labels: Vec<String>,
    properties: BTreeMap<String, String>,
    search_text: String,
    in_degree: usize,
    out_degree: usize,
}

#[derive(Clone, Debug)]
struct ViewerEdge {
    rel_type: String,
}

#[derive(Debug)]
struct GraphModel {
    bundle_root: PathBuf,
    graph: StableDiGraph<ViewerNode, ViewerEdge>,
    positions: HashMap<NodeIndex, Pos2>,
}

impl GraphModel {
    fn from_bundle(bundle: Neo4jExportBundle) -> Self {
        let mut graph = StableDiGraph::new();
        let mut node_map = HashMap::new();

        let mut nodes = bundle.nodes;
        nodes.sort_by(|left, right| left.id.cmp(&right.id));
        for raw in nodes {
            let node = ViewerNode::from_raw(&raw);
            let index = graph.add_node(node);
            node_map.insert(raw.id, index);
        }

        let mut edges = bundle.edges;
        edges.sort_by(|left, right| {
            (&left.source, &left.target, &left.rel_type).cmp(&(
                &right.source,
                &right.target,
                &right.rel_type,
            ))
        });
        for raw in edges {
            let Some(&source) = node_map.get(raw.source.as_str()) else {
                continue;
            };
            let Some(&target) = node_map.get(raw.target.as_str()) else {
                continue;
            };
            graph.add_edge(source, target, ViewerEdge::from_raw(&raw));
        }

        let indices: Vec<_> = graph.node_indices().collect();
        for index in indices {
            let in_degree = graph.edges_directed(index, Direction::Incoming).count();
            let out_degree = graph.edges_directed(index, Direction::Outgoing).count();
            if let Some(node) = graph.node_weight_mut(index) {
                node.in_degree = in_degree;
                node.out_degree = out_degree;
            }
        }

        let positions = build_semantic_layout(&graph);
        Self {
            bundle_root: bundle.root,
            graph,
            positions,
        }
    }

    fn visible_nodes(&self, show_sections: bool, show_citations: bool) -> Vec<NodeIndex> {
        self.graph
            .node_indices()
            .filter(|index| {
                let kind = self.graph[*index].kind.as_str();
                (show_sections || kind != "PaperSection") && (show_citations || kind != "Citation")
            })
            .collect()
    }
}

impl ViewerNode {
    fn from_raw(raw: &Neo4jNode) -> Self {
        let properties = properties_map(&raw.properties);
        let kind = raw
            .labels
            .first()
            .cloned()
            .unwrap_or_else(|| "Node".to_string());
        let title = primary_title(raw.id.as_str(), &kind, &properties);
        let subtitle = subtitle_for_node(&kind, &properties);
        let search_text = build_search_text(raw.id.as_str(), &kind, &properties);

        Self {
            id: raw.id.clone(),
            kind,
            title,
            subtitle,
            labels: raw.labels.clone(),
            properties,
            search_text,
            in_degree: 0,
            out_degree: 0,
        }
    }
}

impl ViewerEdge {
    fn from_raw(raw: &Neo4jEdge) -> Self {
        Self {
            rel_type: raw.rel_type.clone(),
        }
    }
}

struct LitkgViewerApp {
    model: GraphModel,
    selected: Option<NodeIndex>,
    search_query: String,
    show_sections: bool,
    show_citations: bool,
    zoom: f32,
    pan: Vec2,
    fit_to_visible: bool,
    center_selected: bool,
}

impl LitkgViewerApp {
    fn new(bundle: Neo4jExportBundle) -> Self {
        Self {
            model: GraphModel::from_bundle(bundle),
            selected: None,
            search_query: String::new(),
            show_sections: true,
            show_citations: true,
            zoom: 1.0,
            pan: Vec2::ZERO,
            fit_to_visible: true,
            center_selected: false,
        }
    }

    fn draw_side_panel(&mut self, ctx: &egui::Context, visible_nodes: &[NodeIndex]) {
        SidePanel::right("litkg-viewer-sidebar")
            .resizable(true)
            .min_width(320.0)
            .show(ctx, |ui| {
                ui.heading("litkg graph inspector");
                ui.small(self.model.bundle_root.display().to_string());
                ui.separator();

                let total_edges = self.model.graph.edge_count();
                let visible_set: HashSet<_> = visible_nodes.iter().copied().collect();
                let visible_edges = self
                    .model
                    .graph
                    .edge_references()
                    .filter(|edge| {
                        visible_set.contains(&edge.source()) && visible_set.contains(&edge.target())
                    })
                    .count();

                ui.horizontal(|ui| {
                    if ui.button("Fit view").clicked() {
                        self.fit_to_visible = true;
                    }
                    if ui.button("Center selection").clicked() {
                        self.center_selected = true;
                    }
                });
                ui.horizontal(|ui| {
                    let sections_changed =
                        ui.checkbox(&mut self.show_sections, "Sections").changed();
                    let citations_changed =
                        ui.checkbox(&mut self.show_citations, "Citations").changed();
                    if sections_changed || citations_changed {
                        self.fit_to_visible = true;
                    }
                });
                ui.label(format!(
                    "{} / {} nodes visible",
                    visible_nodes.len(),
                    self.model.graph.node_count()
                ));
                ui.label(format!("{visible_edges} / {total_edges} edges visible"));
                ui.separator();

                ui.label("Search");
                ui.text_edit_singleline(&mut self.search_query);
                let search_results = self.search_results(visible_nodes);
                ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                    for index in search_results {
                        let node = &self.model.graph[index];
                        let selected = self.selected == Some(index);
                        if ui.selectable_label(selected, node.title.as_str()).clicked() {
                            self.selected = Some(index);
                            self.center_selected = true;
                        }
                        ui.small(format!("{} · {}", node.kind, node.id));
                        ui.add_space(6.0);
                    }
                });

                ui.separator();
                if let Some(selected) = self.selected {
                    self.draw_node_details(ui, selected);
                } else {
                    ui.label("Select a node to inspect its metadata and neighbors.");
                }
            });
    }

    fn draw_node_details(&mut self, ui: &mut egui::Ui, index: NodeIndex) {
        let node = &self.model.graph[index];
        ui.heading(node.title.as_str());
        if !node.subtitle.is_empty() {
            ui.label(node.subtitle.as_str());
        }
        ui.label(RichText::new(node.kind.as_str()).strong());
        ui.code(node.id.as_str());
        ui.small(format!(
            "{} incoming · {} outgoing",
            node.in_degree, node.out_degree
        ));
        if !node.labels.is_empty() {
            ui.small(format!("labels: {}", node.labels.join(", ")));
        }

        ui.separator();
        ui.collapsing("Properties", |ui| {
            for (key, value) in &node.properties {
                ui.label(RichText::new(key.as_str()).strong());
                ui.label(value.as_str());
                ui.add_space(6.0);
            }
        });

        ui.separator();
        ui.collapsing("Neighbors", |ui| {
            let mut neighbors: Vec<_> = self
                .model
                .graph
                .neighbors_undirected(index)
                .map(|neighbor| {
                    let node = &self.model.graph[neighbor];
                    (node.title.clone(), neighbor)
                })
                .collect();
            neighbors.sort_by(|left, right| left.0.cmp(&right.0));

            for (title, neighbor) in neighbors {
                if ui.button(title.as_str()).clicked() {
                    self.selected = Some(neighbor);
                    self.center_selected = true;
                }
                ui.small(format!(
                    "{} · {}",
                    self.model.graph[neighbor].kind, self.model.graph[neighbor].id
                ));
                ui.add_space(4.0);
            }
        });

        ui.separator();
        ui.collapsing("Connected edges", |ui| {
            let mut edge_rows = Vec::new();
            for edge in self.model.graph.edges_directed(index, Direction::Outgoing) {
                edge_rows.push(format!(
                    "{} -> {}",
                    edge.weight().rel_type,
                    self.model.graph[edge.target()].title
                ));
            }
            for edge in self.model.graph.edges_directed(index, Direction::Incoming) {
                edge_rows.push(format!(
                    "{} <- {}",
                    edge.weight().rel_type,
                    self.model.graph[edge.source()].title
                ));
            }
            edge_rows.sort();
            for row in edge_rows {
                ui.label(row);
            }
        });
    }

    fn search_results(&self, visible_nodes: &[NodeIndex]) -> Vec<NodeIndex> {
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            return visible_nodes.iter().copied().take(16).collect();
        }

        let mut matches: Vec<_> = visible_nodes
            .iter()
            .copied()
            .filter(|index| {
                self.model.graph[*index]
                    .search_text
                    .contains(query.as_str())
            })
            .collect();
        matches.sort_by(|left, right| {
            self.model.graph[*left]
                .title
                .cmp(&self.model.graph[*right].title)
        });
        matches.truncate(24);
        matches
    }

    fn world_to_screen(&self, world: Pos2, rect: Rect) -> Pos2 {
        Pos2::new(
            rect.center().x + self.pan.x + world.x * self.zoom,
            rect.center().y + self.pan.y + world.y * self.zoom,
        )
    }

    fn screen_to_world(&self, screen: Pos2, rect: Rect) -> Pos2 {
        Pos2::new(
            (screen.x - rect.center().x - self.pan.x) / self.zoom,
            (screen.y - rect.center().y - self.pan.y) / self.zoom,
        )
    }

    fn fit_view(&mut self, rect: Rect, visible_nodes: &[NodeIndex]) {
        if visible_nodes.is_empty() {
            return;
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for index in visible_nodes {
            let point = self.model.positions[index];
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }

        let width = (max_x - min_x).max(80.0);
        let height = (max_y - min_y).max(80.0);
        let padding = 120.0;
        let scale_x = (rect.width() - padding).max(120.0) / width;
        let scale_y = (rect.height() - padding).max(120.0) / height;
        self.zoom = scale_x.min(scale_y).clamp(0.08, 2.5);

        let center = Pos2::new((min_x + max_x) * 0.5, (min_y + max_y) * 0.5);
        self.pan = -center.to_vec2() * self.zoom;
    }

    fn center_on_selected(&mut self) {
        let Some(selected) = self.selected else {
            return;
        };
        let Some(position) = self.model.positions.get(&selected) else {
            return;
        };
        self.pan = -position.to_vec2() * self.zoom;
    }

    fn pick_node(
        &self,
        pointer: Pos2,
        rect: Rect,
        visible_nodes: &[NodeIndex],
    ) -> Option<NodeIndex> {
        let mut best = None;
        let mut best_distance = f32::INFINITY;

        for index in visible_nodes {
            let node_screen = self.world_to_screen(self.model.positions[index], rect);
            let radius = self.node_radius(*index) + 8.0;
            let distance = node_screen.distance_sq(pointer);
            if distance <= radius * radius && distance < best_distance {
                best = Some(*index);
                best_distance = distance;
            }
        }
        best
    }

    fn node_radius(&self, index: NodeIndex) -> f32 {
        let node = &self.model.graph[index];
        let base = match node.kind.as_str() {
            "Paper" => 10.0,
            "PaperSection" => 6.0,
            "Citation" => 7.5,
            _ => 7.0,
        };
        base + (node.in_degree + node.out_degree).min(10) as f32 * 0.3
    }
}

impl eframe::App for LitkgViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let visible_nodes = self
            .model
            .visible_nodes(self.show_sections, self.show_citations);
        let visible_set: HashSet<_> = visible_nodes.iter().copied().collect();
        if self.selected.is_some() && !visible_set.contains(&self.selected.unwrap()) {
            self.selected = None;
        }

        self.draw_side_panel(ctx, &visible_nodes);

        TopBottomPanel::top("litkg-viewer-topbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Deterministic native inspector over the litkg Neo4j export bundle.");
                ui.separator();
                ui.label("Pan: drag background");
                ui.separator();
                ui.label("Zoom: mouse wheel");
                ui.separator();
                ui.label("Select: click node or search result");
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let (response, painter) =
                ui.allocate_painter(ui.available_size_before_wrap(), Sense::click_and_drag());
            let rect = response.rect;

            if self.fit_to_visible {
                self.fit_view(rect, &visible_nodes);
                self.fit_to_visible = false;
            }
            if self.center_selected {
                self.center_on_selected();
                self.center_selected = false;
            }

            if response.hovered() {
                let scroll_y = ctx.input(|input| input.raw_scroll_delta.y);
                if scroll_y.abs() > f32::EPSILON {
                    let zoom_factor = (scroll_y / 600.0).exp();
                    if let Some(pointer) = ctx.input(|input| input.pointer.hover_pos()) {
                        let world_before = self.screen_to_world(pointer, rect);
                        self.zoom = (self.zoom * zoom_factor).clamp(0.05, 4.0);
                        let screen_after = self.world_to_screen(world_before, rect);
                        self.pan += pointer - screen_after;
                    }
                }
            }

            if response.dragged() {
                let delta = ctx.input(|input| input.pointer.delta());
                self.pan += delta;
            }

            if response.clicked() {
                if let Some(pointer) = response.interact_pointer_pos() {
                    self.selected = self.pick_node(pointer, rect, &visible_nodes);
                }
            }

            painter.rect_filled(rect, 0.0, Color32::from_rgb(12, 15, 20));
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, Color32::from_gray(40)));

            let selected_neighbors: HashSet<_> = self
                .selected
                .into_iter()
                .flat_map(|selected| self.model.graph.neighbors_undirected(selected))
                .collect();

            for edge in self.model.graph.edge_references() {
                if !visible_set.contains(&edge.source()) || !visible_set.contains(&edge.target()) {
                    continue;
                }

                let selected = self.selected == Some(edge.source())
                    || self.selected == Some(edge.target())
                    || (self.selected == Some(edge.source())
                        && selected_neighbors.contains(&edge.target()))
                    || (self.selected == Some(edge.target())
                        && selected_neighbors.contains(&edge.source()));
                let stroke = Stroke::new(
                    if selected { 2.2 } else { 1.1 },
                    edge_color(edge.weight().rel_type.as_str(), selected),
                );
                let source = self.world_to_screen(self.model.positions[&edge.source()], rect);
                let target = self.world_to_screen(self.model.positions[&edge.target()], rect);
                painter.line_segment([source, target], stroke);
            }

            for index in &visible_nodes {
                let node = &self.model.graph[*index];
                let screen = self.world_to_screen(self.model.positions[index], rect);
                let selected = self.selected == Some(*index);
                let neighbor = selected_neighbors.contains(index);
                let radius = self.node_radius(*index);
                let fill = node_color(node.kind.as_str(), selected, neighbor);
                let stroke = Stroke::new(
                    if selected { 2.5 } else { 1.0 },
                    if selected {
                        Color32::WHITE
                    } else {
                        Color32::from_black_alpha(160)
                    },
                );

                painter.circle_filled(screen, radius, fill);
                painter.circle_stroke(screen, radius, stroke);

                let show_label = selected
                    || node.kind == "Paper"
                    || self.zoom > 0.45
                    || (neighbor && self.zoom > 0.25);
                if show_label {
                    painter.text(
                        screen + Vec2::new(radius + 6.0, 0.0),
                        Align2::LEFT_CENTER,
                        truncate(node.title.as_str(), 72),
                        FontId::proportional(if selected { 14.0 } else { 12.0 }),
                        Color32::from_gray(230),
                    );
                }
            }

            let legend_rect =
                Rect::from_min_size(rect.min + Vec2::new(14.0, 14.0), Vec2::new(220.0, 92.0));
            painter.rect_filled(legend_rect, 8.0, Color32::from_black_alpha(150));
            painter.text(
                legend_rect.min + Vec2::new(12.0, 12.0),
                Align2::LEFT_TOP,
                "Legend",
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            let legend_rows = [
                ("Paper", node_color("Paper", false, false)),
                ("Section", node_color("PaperSection", false, false)),
                ("Citation", node_color("Citation", false, false)),
            ];
            for (row, (label, color)) in legend_rows.into_iter().enumerate() {
                let y = legend_rect.min.y + 36.0 + row as f32 * 18.0;
                let dot = Pos2::new(legend_rect.min.x + 16.0, y);
                painter.circle_filled(dot, 5.5, color);
                painter.text(
                    Pos2::new(legend_rect.min.x + 28.0, y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(11.0),
                    Color32::from_gray(230),
                );
            }
        });
    }
}

fn build_semantic_layout(
    graph: &StableDiGraph<ViewerNode, ViewerEdge>,
) -> HashMap<NodeIndex, Pos2> {
    let mut positions = HashMap::new();
    let mut papers: Vec<_> = graph
        .node_indices()
        .filter(|index| graph[*index].kind == "Paper")
        .collect();
    papers.sort_by(|left, right| graph[*left].title.cmp(&graph[*right].title));

    let paper_count = papers.len().max(1) as f32;
    let paper_ring = if papers.len() <= 1 {
        0.0
    } else {
        260.0 + 48.0 * paper_count.sqrt()
    };

    for (offset, paper) in papers.iter().enumerate() {
        let angle = if papers.len() <= 1 {
            0.0
        } else {
            offset as f32 / paper_count * TAU
        };
        let paper_pos = Pos2::new(angle.cos() * paper_ring, angle.sin() * paper_ring);
        positions.insert(*paper, paper_pos);

        let mut sections: Vec<_> = graph
            .edges(*paper)
            .filter(|edge| edge.weight().rel_type == "HAS_SECTION")
            .map(|edge| edge.target())
            .collect();
        sections.sort_by(|left, right| graph[*left].title.cmp(&graph[*right].title));

        let outward = normalized_or(
            paper_pos.to_vec2(),
            if papers.len() <= 1 { Vec2::X } else { Vec2::Y },
        );
        let tangent = Vec2::new(-outward.y, outward.x);
        let center_offset = (sections.len() as f32 - 1.0) * 0.5;
        for (section_index, section) in sections.iter().enumerate() {
            let lane = section_index as f32 - center_offset;
            let radial = 150.0 + (section_index / 6) as f32 * 18.0;
            let lateral = lane * 38.0;
            let section_pos = paper_pos + outward * radial + tangent * lateral;
            positions.insert(*section, section_pos);
        }
    }

    let mut citations: Vec<_> = graph
        .node_indices()
        .filter(|index| graph[*index].kind == "Citation")
        .collect();
    citations.sort_by(|left, right| graph[*left].title.cmp(&graph[*right].title));

    for citation in citations {
        let parents: Vec<_> = graph
            .edges_directed(citation, Direction::Incoming)
            .filter(|edge| edge.weight().rel_type == "CITES")
            .map(|edge| edge.source())
            .filter(|source| graph[*source].kind == "Paper")
            .collect();

        if parents.is_empty() {
            let angle = hash_angle(graph[citation].id.as_str());
            positions.insert(
                citation,
                Pos2::new(
                    angle.cos() * (paper_ring + 220.0),
                    angle.sin() * (paper_ring + 220.0),
                ),
            );
            continue;
        }

        let centroid = parents
            .iter()
            .map(|index| positions.get(index).copied().unwrap_or(Pos2::ZERO))
            .fold(Vec2::ZERO, |acc, pos| acc + pos.to_vec2())
            / parents.len() as f32;
        let base = Pos2::new(centroid.x, centroid.y);
        let outward = normalized_or(base.to_vec2(), Vec2::new(1.0, 0.0));
        let tangent = Vec2::new(-outward.y, outward.x);
        let jitter = hash_unit(graph[citation].id.as_str());
        let citation_pos = base + outward * 220.0 + tangent * ((jitter - 0.5) * 140.0);
        positions.insert(citation, citation_pos);
    }

    let mut spill_index = 0usize;
    let mut leftovers: Vec<_> = graph
        .node_indices()
        .filter(|index| !positions.contains_key(index))
        .collect();
    leftovers.sort_by(|left, right| graph[*left].title.cmp(&graph[*right].title));
    for leftover in leftovers {
        let row = spill_index / 8;
        let col = spill_index % 8;
        positions.insert(
            leftover,
            Pos2::new(
                -280.0 + col as f32 * 90.0,
                paper_ring + 260.0 + row as f32 * 70.0,
            ),
        );
        spill_index += 1;
    }

    positions
}

fn primary_title(id: &str, kind: &str, properties: &BTreeMap<String, String>) -> String {
    match kind {
        "Paper" | "PaperSection" => properties
            .get("title")
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        "Citation" => properties
            .get("citation_key")
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
        _ => properties
            .get("title")
            .cloned()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| id.to_string()),
    }
}

fn subtitle_for_node(kind: &str, properties: &BTreeMap<String, String>) -> String {
    match kind {
        "Paper" => {
            let mut parts = Vec::new();
            if let Some(year) = properties.get("year").filter(|value| !value.is_empty()) {
                parts.push(year.clone());
            }
            if let Some(arxiv) = properties.get("arxiv_id").filter(|value| !value.is_empty()) {
                parts.push(format!("arXiv:{arxiv}"));
            }
            parts.join(" · ")
        }
        "PaperSection" => {
            let mut parts = Vec::new();
            if let Some(level) = properties.get("level").filter(|value| !value.is_empty()) {
                parts.push(format!("Level {level}"));
            }
            if let Some(paper_id) = properties.get("paper_id").filter(|value| !value.is_empty()) {
                parts.push(paper_id.clone());
            }
            parts.join(" · ")
        }
        "Citation" => properties
            .get("citation_key")
            .cloned()
            .unwrap_or_else(String::new),
        _ => String::new(),
    }
}

fn build_search_text(id: &str, kind: &str, properties: &BTreeMap<String, String>) -> String {
    let mut parts = vec![id.to_lowercase(), kind.to_lowercase()];
    for key in [
        "title",
        "paper_id",
        "citation_key",
        "arxiv_id",
        "year",
        "content",
    ] {
        if let Some(value) = properties.get(key) {
            let snippet = if key == "content" {
                truncate(value.as_str(), 220)
            } else {
                value.clone()
            };
            parts.push(snippet.to_lowercase());
        }
    }
    parts.join(" ")
}

fn properties_map(value: &Value) -> BTreeMap<String, String> {
    match value {
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| (key.clone(), json_value_string(value)))
            .collect(),
        _ => {
            let mut properties = BTreeMap::new();
            properties.insert("value".to_string(), json_value_string(value));
            properties
        }
    }
}

fn json_value_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        _ => value.to_string(),
    }
}

fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_string();
    }
    let clipped: String = value.chars().take(max_len.saturating_sub(1)).collect();
    format!("{clipped}…")
}

fn hash_angle(value: &str) -> f32 {
    hash_unit(value) * TAU
}

fn hash_unit(value: &str) -> f32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    let hash = hasher.finish();
    hash as f32 / u64::MAX as f32
}

fn normalized_or(value: Vec2, fallback: Vec2) -> Vec2 {
    if value.length_sq() > 1e-6 {
        value.normalized()
    } else {
        fallback.normalized()
    }
}

fn node_color(kind: &str, selected: bool, neighbor: bool) -> Color32 {
    let mut color = match kind {
        "Paper" => Color32::from_rgb(78, 121, 167),
        "PaperSection" => Color32::from_rgb(89, 161, 79),
        "Citation" => Color32::from_rgb(237, 201, 72),
        _ => Color32::from_rgb(176, 122, 161),
    };

    if neighbor {
        color = color.gamma_multiply(1.1);
    }
    if selected {
        color = color.gamma_multiply(1.35);
    }
    color
}

fn edge_color(rel_type: &str, selected: bool) -> Color32 {
    let color = match rel_type {
        "HAS_SECTION" => Color32::from_rgb(92, 140, 94),
        "CITES" => Color32::from_rgb(198, 170, 64),
        _ => Color32::from_gray(130),
    };
    if selected {
        color.gamma_multiply(1.35)
    } else {
        color.gamma_multiply(0.8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use litkg_neo4j::{Neo4jEdge, Neo4jExportBundle, Neo4jNode};

    fn sample_bundle() -> Neo4jExportBundle {
        Neo4jExportBundle {
            root: PathBuf::from("/tmp/litkg-viewer-test"),
            nodes: vec![
                Neo4jNode {
                    id: "paper:alpha".into(),
                    labels: vec!["Paper".into()],
                    properties: serde_json::json!({
                        "paper_id": "alpha",
                        "title": "Alpha SLAM",
                        "year": "2025",
                    }),
                },
                Neo4jNode {
                    id: "paper:alpha:section:0".into(),
                    labels: vec!["PaperSection".into()],
                    properties: serde_json::json!({
                        "paper_id": "alpha",
                        "title": "Introduction",
                        "level": 1,
                    }),
                },
                Neo4jNode {
                    id: "citation:foo".into(),
                    labels: vec!["Citation".into()],
                    properties: serde_json::json!({
                        "citation_key": "foo2025"
                    }),
                },
            ],
            edges: vec![
                Neo4jEdge {
                    source: "paper:alpha".into(),
                    target: "paper:alpha:section:0".into(),
                    rel_type: "HAS_SECTION".into(),
                    properties: serde_json::json!({}),
                },
                Neo4jEdge {
                    source: "paper:alpha".into(),
                    target: "citation:foo".into(),
                    rel_type: "CITES".into(),
                    properties: serde_json::json!({}),
                },
            ],
        }
    }

    #[test]
    fn builds_graph_model_from_export_bundle() {
        let model = GraphModel::from_bundle(sample_bundle());
        assert_eq!(model.graph.node_count(), 3);
        assert_eq!(model.graph.edge_count(), 2);
        assert_eq!(
            model
                .graph
                .node_indices()
                .filter(|index| model.graph[*index].kind == "Paper")
                .count(),
            1
        );
    }

    #[test]
    fn semantic_layout_is_deterministic_for_same_bundle() {
        let first = GraphModel::from_bundle(sample_bundle());
        let second = GraphModel::from_bundle(sample_bundle());
        assert_eq!(first.positions, second.positions);
    }
}
