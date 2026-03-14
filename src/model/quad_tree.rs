use glm::Vec2;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Copy, Clone)]
pub struct Rect {
    pub pos: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub fn new() -> Self {
        Self {
            pos: Vec2::new(0.0, 0.0),
            size: Vec2::new(1.0, 1.0),
        }
    }

    pub fn contains_point(&self, p: &Vec2) -> bool {
        return !(p.x < self.pos.x
            || p.x > (self.pos.x + self.size.x)
            || p.y < self.pos.y
            || p.y > (self.pos.y + self.size.y));
    }

    pub fn contains_rect(&self, r: &Rect) -> bool {
        return (self.pos.x <= r.pos.x)
            && (self.pos.x + self.pos.x > r.pos.x + r.size.x)
            && (self.pos.y <= r.pos.y)
            && (self.pos.y + self.pos.y > r.pos.y + r.size.y);
    }

    pub fn overlaps_rect(&self, r: &Rect) -> bool {
        self.pos.x < r.pos.x + r.size.x
            && self.pos.x + self.size.x > r.pos.x
            && self.pos.y < r.pos.y + r.size.y
            && self.pos.y + self.size.y > r.pos.y
    }
}

struct QuadTreeItemLocation {
    container: Rc<RefCell<Vec<(Rect, usize)>>>,
    id: usize,
}

struct QuadTreeItem<T> {
    item: T,
    loc: QuadTreeItemLocation,
}

pub struct QuadTreeContainer<T> {
    items: Vec<QuadTreeItem<T>>,
    root: QuadTree,
}

impl<T> QuadTreeContainer<T> {
    pub fn new(area: Rect, depth: usize) -> Self {
        Self {
            items: Vec::new(),
            root: QuadTree::new_with_size(area, depth),
        }
    }

    pub fn insert(&mut self, item: T, area: Rect) {
        self.items.push(QuadTreeItem {
            item: item,
            loc: self.root.insert(self.items.len(), area),
        });
    }

    pub fn search(&self, area: &Rect) -> Vec<&T> {
        let indices: Vec<usize> = self.root.search(area);
        indices.iter().map(|&i| &self.items[i].item).collect()
    }

    pub fn remove(&mut self, index: usize) {
        // +-------------+---------------------------------+---------------------------------+
        // | Inner\Outer |             Middle              |              Tail               |
        // +-------------+---------------------------------+---------------------------------+
        // | Middle      | Change Both                     | Update Inner List + Outer index |
        // | Tail        | Update Inner List + Inner Index | Just remove                     |
        // +-------------+---------------------------------+---------------------------------+

        // First remove elem from its inner list, and get back a possible tail index
        let tail_old_index = 
        {
            let mut inner_list = self.items[index].loc.container.borrow_mut();
            let inner_index = self.items[index].loc.id;

            if inner_index == inner_list.len() - 1 {
                inner_list.pop();
                None
            } else {
                inner_list.swap_remove(inner_index);
                Some(inner_list[inner_index].1)
            }
        };
        // Correcting any outer list updates
        if let Some(id) = tail_old_index {
            self.items[id].loc.id = self.items[index].loc.id;
        }

        // Next we change the outer list
        if index == self.items.len() - 1 {
            self.items.pop();
        } else {
            self.items.swap_remove(index);
            let swapped_item = &self.items[index];
            let inner_list = &mut swapped_item.loc.container.borrow_mut();
            inner_list[swapped_item.loc.id].1 = index;
        }
    }
}

struct QuadTree {
    depth: usize,
    rect: Rect,
    child_rects: [Rect; 4],
    child_nodes: [Option<Box<QuadTree>>; 4],
    items: Rc<RefCell<Vec<(Rect, usize)>>>,
}

impl QuadTree {
    pub fn new(depth: usize) -> Self {
        QuadTree {
            depth: depth,
            rect: Rect {
                pos: Vec2::new(0.0, 0.0),
                size: Vec2::new(0.0, 0.0),
            },
            child_rects: [Rect::new(); 4],
            child_nodes: [None, None, None, None],
            items: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn new_with_size(area: Rect, depth: usize) -> Self {
        let mut tree = Self::new(depth);
        tree.resize(&area);
        tree
    }

    fn resize(&mut self, a: &Rect) {
        self.clear();
        self.rect.size = a.size;
        self.rect.pos = a.pos;

        // Setting up children size
        let child_size = self.rect.size / 2.0;
        self.child_rects = [
            Rect {
                pos: a.pos,
                size: child_size,
            },
            Rect {
                pos: Vec2::new(a.pos.x + child_size.x, a.pos.y),
                size: child_size,
            },
            Rect {
                pos: Vec2::new(a.pos.x, a.pos.y + child_size.y),
                size: child_size,
            },
            Rect {
                pos: a.pos + child_size,
                size: child_size,
            },
        ];
    }

    fn clear(&mut self) {
        self.items.borrow_mut().clear();

        for node in self.child_nodes.iter_mut().flatten() {
            node.clear();
        }
    }

    pub fn size(&self) -> usize {
        self.items.borrow_mut().len()
            + self
                .child_nodes
                .iter()
                .flatten()
                .map(|node| node.size())
                .sum::<usize>()
    }

    pub fn insert(&mut self, item: usize, area: Rect) -> QuadTreeItemLocation {
        for i in 0..4 {
            if self.child_rects[i].contains_rect(&area) {
                if self.depth > 0 {
                    let child = self.child_nodes[i].get_or_insert_with(|| {
                        Box::new(QuadTree::new_with_size(
                            self.child_rects[i].clone(),
                            self.depth - 1,
                        ))
                    });
                    return child.insert(item, area);
                }
            }
        }
        self.items.borrow_mut().push((area, item));
        let len = self.items.borrow().len() - 1;
        QuadTreeItemLocation {
            container: self.items.clone(),
            id: len,
        }
    }

    pub fn search(&self, area: &Rect) -> Vec<usize> {
        let mut items: Vec<usize> = Vec::new();
        self.search_list(area, &mut items);
        items
    }

    fn search_list(&self, area: &Rect, item_list: &mut Vec<usize>) {
        for (i, j) in self.items.borrow_mut().iter() {
            if i.overlaps_rect(area) {
                item_list.push(j.clone());
            }
        }

        for i in 0..4 {
            match &self.child_nodes[i] {
                None => {}
                Some(tree) => {
                    if area.contains_rect(&(tree.rect)) {
                        tree.list_items(item_list);
                    } else if area.overlaps_rect(&(tree.rect)) {
                        tree.search_list(area, item_list);
                    }
                }
            }
        }
    }

    pub fn list_items(&self, item_list: &mut Vec<usize>) {
        for i in self.items.borrow_mut().iter() {
            item_list.push(i.1.clone());
        }

        for i in self.child_nodes.iter().flatten() {
            i.list_items(item_list);
        }
    }

    pub fn area(&self) -> &Rect {
        &(self.rect)
    }
}
