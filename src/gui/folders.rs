//! The folders the grouped images are in, as a tree under the folders the
//! scan was given, for the results page's tree view.
//!
//! Only folders holding a grouped image, or a folder below that holds one,
//! are in it: the page knows nothing of the other files a scan read. Each
//! folder lists every image under it, its subfolders' included, in path
//! order, since that is what the page shows when it is selected.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct Folder {
    pub path: PathBuf,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// The images directly in it.
    pub own: usize,
    /// Every image in it and below it, by number, in path order.
    pub all: Vec<u32>,
}

impl Folder {
    /// What the tree calls it: a top folder by its whole path, any other by
    /// its name.
    pub fn label(&self) -> String {
        match (self.parent, self.path.file_name()) {
            (Some(_), Some(n)) => n.to_string_lossy().into_owned(),
            _ => self.path.display().to_string(),
        }
    }
}

pub struct Tree {
    pub folders: Vec<Folder>,
    /// The top folders, in order.
    pub top: Vec<usize>,
}

impl Tree {
    /// The folder at `path`, if the tree has one.
    pub fn find(&self, path: &Path) -> Option<usize> {
        self.folders.iter().position(|f| f.path == path)
    }

    /// `i`'s ancestors, the top folder first, `i` itself not included.
    pub fn ancestors(&self, i: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut at = self.folders.get(i).and_then(|f| f.parent);
        while let Some(p) = at {
            chain.push(p);
            at = self.folders[p].parent;
        }
        chain.reverse();
        chain
    }
}

/// The tree of `files`, numbered paths, under `roots`, the folders the scan
/// was given. A root inside another is the other's subfolder rather than a
/// top folder of its own. Files under no root (a scan kept by a window that
/// did not record its folders) go under the deepest folder they all share.
pub fn build(roots: &[PathBuf], files: &[(u32, &Path)]) -> Tree {
    let mut tops: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !roots.iter().any(|o| o != r && r.starts_with(o)) && !tops.contains(r) {
            tops.push(r.clone());
        }
    }
    let mut files: Vec<(u32, &Path)> = files.to_vec();
    files.sort_by(|a, b| a.1.cmp(b.1));
    let dir = |p: &Path| p.parent().map(Path::to_path_buf).unwrap_or_default();
    let stray: Vec<PathBuf> = files.iter().map(|(_, p)| dir(p)).filter(|d| !tops.iter().any(|t| d.starts_with(t))).collect();
    if let Some(first) = stray.first() {
        let mut common = first.clone();
        for d in &stray[1..] {
            while !d.starts_with(&common) {
                if !common.pop() {
                    break;
                }
            }
        }
        tops.push(common);
    }

    let mut t = Tree { folders: Vec::new(), top: Vec::new() };
    let mut at: HashMap<PathBuf, usize> = HashMap::new();
    for (id, path) in files {
        let d = dir(path);
        let Some(top) = tops.iter().filter(|t| d.starts_with(t)).max_by_key(|t| t.components().count()) else { continue };
        // The chain of folders from the top one down to the file's, made as
        // they are first met: in path order, so each folder's children come
        // in the order their names sort.
        let mut chain = vec![d.clone()];
        let mut up = d.clone();
        while up != *top && up.pop() {
            chain.push(up.clone());
        }
        let mut parent: Option<usize> = None;
        for p in chain.into_iter().rev() {
            let i = match at.get(&p) {
                Some(&i) => i,
                None => {
                    let i = t.folders.len();
                    t.folders.push(Folder { path: p.clone(), parent, children: Vec::new(), own: 0, all: Vec::new() });
                    match parent {
                        Some(q) => t.folders[q].children.push(i),
                        None => t.top.push(i),
                    }
                    at.insert(p, i);
                    i
                }
            };
            t.folders[i].all.push(id);
            parent = Some(i);
        }
        if let Some(i) = parent {
            t.folders[i].own += 1;
        }
    }
    // The top folders in the order the scan was given them.
    t.top.sort_by_key(|&i| tops.iter().position(|r| *r == t.folders[i].path));
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(roots: &[&str], files: &[&str]) -> Tree {
        let roots: Vec<PathBuf> = roots.iter().map(PathBuf::from).collect();
        let paths: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        let files: Vec<(u32, &Path)> = paths.iter().enumerate().map(|(i, p)| (i as u32, p.as_path())).collect();
        build(&roots, &files)
    }

    fn names(t: &Tree, of: &[usize]) -> Vec<String> {
        of.iter().map(|&i| t.folders[i].label()).collect()
    }

    #[test]
    fn a_folder_holds_its_subfolders_images_in_path_order() {
        let t = tree(&["/p"], &["/p/b/2.jpg", "/p/a.jpg", "/p/b/1.jpg", "/p/b/c/3.jpg", "/p/a/4.jpg"]);
        assert_eq!(names(&t, &t.top), ["/p"]);
        let root = &t.folders[t.top[0]];
        assert_eq!(root.own, 1);
        assert_eq!(root.all, [4, 1, 2, 0, 3]);
        assert_eq!(names(&t, &root.children), ["a", "b"]);
        let b = &t.folders[root.children[1]];
        assert_eq!((b.own, b.all.clone()), (2, vec![2, 0, 3]));
        assert_eq!(names(&t, &b.children), ["c"]);
        let c = t.find(Path::new("/p/b/c")).unwrap();
        assert_eq!(t.ancestors(c), [t.top[0], root.children[1]]);
    }

    #[test]
    fn the_root_is_the_folder_given_even_with_no_image_of_its_own() {
        let t = tree(&["/p"], &["/p/x/y/1.jpg", "/p/x/y/2.jpg"]);
        assert_eq!(names(&t, &t.top), ["/p"]);
        assert_eq!(t.folders[t.top[0]].all.len(), 2);
        assert_eq!(t.folders.len(), 3);
    }

    #[test]
    fn roots_nested_and_missing_are_settled() {
        // `/p/q` is inside `/p`, and `/r` holds nothing.
        let t = tree(&["/r", "/p/q", "/p"], &["/p/q/1.jpg", "/p/2.jpg"]);
        assert_eq!(names(&t, &t.top), ["/p"]);
        // No roots recorded: the deepest folder the files share.
        let t = tree(&[], &["/h/u/a/1.jpg", "/h/u/b/2.jpg"]);
        assert_eq!(names(&t, &t.top), ["/h/u"]);
        assert_eq!(names(&t, &t.folders[t.top[0]].children), ["a", "b"]);
    }
}
