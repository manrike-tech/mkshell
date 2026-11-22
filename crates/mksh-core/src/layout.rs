//! Árbol binario de paneles de una sesión.
//!
//! Cada hoja es un panel y cada nodo interno una división con su proporción.
//! Vive en el núcleo (no en la UI) para que el futuro daemon pueda restaurar el
//! layout y para que la navegación entre paneles sea testeable sin interfaz.

use crate::{CoreError, command::SplitDirection, ids::PaneId, ids::SplitId};
use serde::{Deserialize, Serialize};

const MIN_RATIO: f32 = 0.05;
const MAX_RATIO: f32 = 0.95;
const EPS: f32 = 1e-4;

/// Nodo del árbol de layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LayoutNode {
    Pane {
        id: PaneId,
    },
    Split {
        id: SplitId,
        direction: SplitDirection,
        /// Fracción del espacio para `first` (izquierda o arriba), entre 0.05 y 0.95.
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

/// Dirección de navegación entre paneles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// Rectángulo en coordenadas normalizadas (0..1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const UNIT: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
}

/// Árbol de layout de una sesión.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutTree {
    pub root: LayoutNode,
}

impl LayoutTree {
    /// Árbol con un único panel.
    pub fn new(pane: PaneId) -> Self {
        Self {
            root: LayoutNode::Pane { id: pane },
        }
    }

    /// Paneles en orden de lectura (izquierda→derecha, arriba→abajo).
    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        collect(&self.root, &mut out);
        out
    }

    pub fn contains(&self, pane: PaneId) -> bool {
        self.panes().contains(&pane)
    }

    /// Divide `target` y coloca `new_pane` a la derecha (vertical) o debajo (horizontal).
    pub fn split(
        &mut self,
        target: PaneId,
        direction: SplitDirection,
        new_pane: PaneId,
    ) -> Result<SplitId, CoreError> {
        let node = find_leaf_mut(&mut self.root, target).ok_or(CoreError::PaneNotFound(target))?;
        let id = SplitId::new();
        let old = std::mem::replace(node, LayoutNode::Pane { id: new_pane });
        *node = LayoutNode::Split {
            id,
            direction,
            ratio: 0.5,
            first: Box::new(old),
            second: Box::new(LayoutNode::Pane { id: new_pane }),
        };
        Ok(id)
    }

    /// Elimina un panel; su hermano ocupa el espacio de la división.
    pub fn remove(&mut self, pane: PaneId) -> Result<(), CoreError> {
        if let LayoutNode::Pane { id } = &self.root {
            return if *id == pane {
                Err(CoreError::LastPane)
            } else {
                Err(CoreError::PaneNotFound(pane))
            };
        }
        if remove_from(&mut self.root, pane) {
            Ok(())
        } else {
            Err(CoreError::PaneNotFound(pane))
        }
    }

    /// Cambia la proporción de una división (se acota a 0.05..0.95).
    pub fn set_ratio(&mut self, split: SplitId, ratio: f32) -> Result<(), CoreError> {
        let node = find_split_mut(&mut self.root, split).ok_or(CoreError::SplitNotFound(split))?;
        if let LayoutNode::Split { ratio: r, .. } = node {
            *r = ratio.clamp(MIN_RATIO, MAX_RATIO);
        }
        Ok(())
    }

    /// Rectángulos de cada panel dentro de `area`.
    pub fn rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        layout_rects(&self.root, area, &mut out);
        out
    }

    /// Panel vecino en una dirección, si existe.
    ///
    /// Se elige el candidato más cercano en esa dirección que se solape en el eje
    /// perpendicular; a igual distancia gana el de mayor solape.
    pub fn neighbor(&self, pane: PaneId, dir: Direction) -> Option<PaneId> {
        let rects = self.rects(Rect::UNIT);
        let (_, from) = rects.iter().find(|(id, _)| *id == pane)?;
        let mut best: Option<(f32, f32, PaneId)> = None;
        for (id, r) in &rects {
            if *id == pane {
                continue;
            }
            let (dist, overlap) = match dir {
                Direction::Left if r.x + r.w <= from.x + EPS => {
                    (from.x - (r.x + r.w), overlap(from.y, from.h, r.y, r.h))
                }
                Direction::Right if r.x >= from.x + from.w - EPS => {
                    (r.x - (from.x + from.w), overlap(from.y, from.h, r.y, r.h))
                }
                Direction::Up if r.y + r.h <= from.y + EPS => {
                    (from.y - (r.y + r.h), overlap(from.x, from.w, r.x, r.w))
                }
                Direction::Down if r.y >= from.y + from.h - EPS => {
                    (r.y - (from.y + from.h), overlap(from.x, from.w, r.x, r.w))
                }
                _ => continue,
            };
            if overlap <= EPS {
                continue;
            }
            let better = match best {
                None => true,
                Some((bd, bo, _)) => dist < bd - EPS || ((dist - bd).abs() <= EPS && overlap > bo),
            };
            if better {
                best = Some((dist, overlap, *id));
            }
        }
        best.map(|(_, _, id)| id)
    }
}

fn overlap(a: f32, alen: f32, b: f32, blen: f32) -> f32 {
    (a + alen).min(b + blen) - a.max(b)
}

fn collect(node: &LayoutNode, out: &mut Vec<PaneId>) {
    match node {
        LayoutNode::Pane { id } => out.push(*id),
        LayoutNode::Split { first, second, .. } => {
            collect(first, out);
            collect(second, out);
        }
    }
}

fn find_leaf_mut(node: &mut LayoutNode, pane: PaneId) -> Option<&mut LayoutNode> {
    match node {
        LayoutNode::Pane { id } if *id == pane => Some(node),
        LayoutNode::Pane { .. } => None,
        LayoutNode::Split { first, second, .. } => {
            find_leaf_mut(first, pane).or_else(|| find_leaf_mut(second, pane))
        }
    }
}

fn find_split_mut(node: &mut LayoutNode, split: SplitId) -> Option<&mut LayoutNode> {
    match node {
        LayoutNode::Pane { .. } => None,
        LayoutNode::Split { id, .. } if *id == split => Some(node),
        LayoutNode::Split { first, second, .. } => {
            find_split_mut(first, split).or_else(|| find_split_mut(second, split))
        }
    }
}

/// Elimina `pane` de un subárbol que es una división. Devuelve `true` si lo encontró.
fn remove_from(node: &mut LayoutNode, pane: PaneId) -> bool {
    let LayoutNode::Split { first, second, .. } = node else {
        return false;
    };
    let is_leaf = |n: &LayoutNode| matches!(n, LayoutNode::Pane { id } if *id == pane);
    if is_leaf(first) {
        let sibling = std::mem::replace(second.as_mut(), LayoutNode::Pane { id: pane });
        *node = sibling;
        return true;
    }
    if is_leaf(second) {
        let sibling = std::mem::replace(first.as_mut(), LayoutNode::Pane { id: pane });
        *node = sibling;
        return true;
    }
    remove_from(first, pane) || remove_from(second, pane)
}

fn layout_rects(node: &LayoutNode, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
    match node {
        LayoutNode::Pane { id } => out.push((*id, area)),
        LayoutNode::Split {
            direction,
            ratio,
            first,
            second,
            ..
        } => {
            let (a, b) = match direction {
                SplitDirection::Vertical => {
                    let w1 = area.w * ratio;
                    (
                        Rect { w: w1, ..area },
                        Rect {
                            x: area.x + w1,
                            w: area.w - w1,
                            ..area
                        },
                    )
                }
                SplitDirection::Horizontal => {
                    let h1 = area.h * ratio;
                    (
                        Rect { h: h1, ..area },
                        Rect {
                            y: area.y + h1,
                            h: area.h - h1,
                            ..area
                        },
                    )
                }
            };
            layout_rects(first, a, out);
            layout_rects(second, b, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Construye:
    /// ```text
    /// +-----+-----+
    /// |  a  |  b  |
    /// |     +-----+
    /// |     |  c  |
    /// +-----+-----+
    /// ```
    fn sample() -> (LayoutTree, PaneId, PaneId, PaneId) {
        let (a, b, c) = (PaneId::new(), PaneId::new(), PaneId::new());
        let mut t = LayoutTree::new(a);
        t.split(a, SplitDirection::Vertical, b).unwrap();
        t.split(b, SplitDirection::Horizontal, c).unwrap();
        (t, a, b, c)
    }

    #[test]
    fn split_and_order() {
        let (t, a, b, c) = sample();
        assert_eq!(t.panes(), vec![a, b, c]);
    }

    #[test]
    fn rects_cover_area() {
        let (t, a, b, c) = sample();
        let r = t.rects(Rect::UNIT);
        let get = |p| r.iter().find(|(id, _)| *id == p).unwrap().1;
        assert_eq!(
            get(a),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 1.0
            }
        );
        assert_eq!(
            get(b),
            Rect {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 0.5
            }
        );
        assert_eq!(
            get(c),
            Rect {
                x: 0.5,
                y: 0.5,
                w: 0.5,
                h: 0.5
            }
        );
    }

    #[test]
    fn neighbors() {
        let (t, a, b, c) = sample();
        assert_eq!(t.neighbor(a, Direction::Right), Some(b));
        assert_eq!(t.neighbor(c, Direction::Left), Some(a));
        assert_eq!(t.neighbor(b, Direction::Down), Some(c));
        assert_eq!(t.neighbor(c, Direction::Up), Some(b));
        assert_eq!(t.neighbor(a, Direction::Left), None);
        assert_eq!(t.neighbor(b, Direction::Up), None);
    }

    #[test]
    fn remove_promotes_sibling() {
        let (mut t, a, b, c) = sample();
        t.remove(b).unwrap();
        assert_eq!(t.panes(), vec![a, c]);
        t.remove(a).unwrap();
        assert_eq!(t.root, LayoutNode::Pane { id: c });
        assert!(matches!(t.remove(c), Err(CoreError::LastPane)));
        assert!(matches!(t.remove(a), Err(CoreError::PaneNotFound(_))));
    }

    #[test]
    fn ratio_is_clamped() {
        let (mut t, _, _, _) = sample();
        let LayoutNode::Split { id, .. } = t.root.clone() else {
            panic!()
        };
        t.set_ratio(id, 2.0).unwrap();
        let LayoutNode::Split { ratio, .. } = t.root else {
            panic!()
        };
        assert_eq!(ratio, MAX_RATIO);
    }

    #[test]
    fn serializes_for_the_ui() {
        let p = PaneId(7);
        let t = LayoutTree::new(p);
        assert_eq!(
            serde_json::to_string(&t.root).unwrap(),
            r#"{"type":"pane","id":7}"#
        );
    }
}
