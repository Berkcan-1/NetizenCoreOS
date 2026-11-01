extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use crate::vfs::{DirEntry, FileStat, FileSystem, FileType, FsError};

#[derive(Clone, Debug)]
pub struct MemNode {
    pub name: String,
    pub is_dir: bool,
    pub content: Vec<u8>,
    pub children: Vec<MemNode>,
    pub id: u64,
}

pub struct MemFs {
    root: MemNode,
    next_id: u64,
}

impl MemFs {
    pub fn new() -> Self {
        let mut fs = MemFs {
            root: MemNode {
                name: String::from(""),
                is_dir: true,
                content: Vec::new(),
                children: Vec::new(),
                id: 1,
            },
            next_id: 100,
        };

        let _ = fs.create_dir("documents");
        let _ = fs.create_dir("pictures");
        let _ = fs.create_dir("downloads");

        let _ = fs.write_file(
            "documents/welcome.txt",
            b"Welcome to NetizenCore OS!\nThis is a fully custom operating system developed from scratch in Rust.\n\nYou can create, edit, and delete files in this RAM disk.",
        );
        let _ = fs.write_file(
            "documents/notes.txt",
            b"NetizenCore File System Architecture:\n- Unified Virtual File System (VFS)\n- EROFS system partition (/system)\n- exFAT data storage (/data)\n- Writable MemFS RAM drive (/ram)",
        );

        fs
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn find_node_mut<'a>(&'a mut self, path: &str) -> Result<&'a mut MemNode, FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(&mut self.root);
        }

        let mut curr = &mut self.root;
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                continue;
            }
            if !curr.is_dir {
                return Err(FsError::NotADirectory);
            }
            let pos = curr.children.iter().position(|child| child.name == segment);
            match pos {
                Some(idx) => {
                    curr = &mut curr.children[idx];
                }
                None => return Err(FsError::NotFound),
            }
        }
        Ok(curr)
    }

    fn find_node<'a>(&'a self, path: &str) -> Result<&'a MemNode, FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Ok(&self.root);
        }

        let mut curr = &self.root;
        for segment in trimmed.split('/') {
            if segment.is_empty() {
                continue;
            }
            if !curr.is_dir {
                return Err(FsError::NotADirectory);
            }
            match curr.children.iter().find(|child| child.name == segment) {
                Some(node) => curr = node,
                None => return Err(FsError::NotFound),
            }
        }
        Ok(curr)
    }

    fn split_parent_and_name<'a>(&self, path: &'a str) -> Result<(&'a str, &'a str), FsError> {
        let trimmed = path.trim_matches('/');
        if trimmed.is_empty() {
            return Err(FsError::InvalidPath);
        }

        if let Some(pos) = trimmed.rfind('/') {
            let parent = &trimmed[..pos];
            let name = &trimmed[pos + 1..];
            if name.is_empty() {
                Err(FsError::InvalidPath)
            } else {
                Ok((parent, name))
            }
        } else {
            Ok(("", trimmed))
        }
    }
}

impl FileSystem for MemFs {
    fn list_dir(&mut self, path: &str) -> Result<Vec<DirEntry>, FsError> {
        let node = self.find_node(path)?;
        if !node.is_dir {
            return Err(FsError::NotADirectory);
        }

        let mut entries = Vec::new();
        for child in &node.children {
            entries.push(DirEntry {
                name: child.name.clone(),
                file_type: if child.is_dir {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
                size: child.content.len() as u64,
                id: child.id,
            });
        }
        Ok(entries)
    }

    fn read_file(&mut self, path: &str) -> Result<Vec<u8>, FsError> {
        let node = self.find_node(path)?;
        if node.is_dir {
            return Err(FsError::NotAFile);
        }
        Ok(node.content.clone())
    }

    fn read_file_range(
        &mut self,
        path: &str,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize, FsError> {
        let node = self.find_node(path)?;
        if node.is_dir {
            return Err(FsError::NotAFile);
        }
        if output.is_empty() {
            return Ok(0);
        }
        let start = usize::try_from(offset).map_err(|_| FsError::Corrupted)?;
        if start >= node.content.len() {
            return Ok(0);
        }
        let count = output.len().min(node.content.len() - start);
        output[..count].copy_from_slice(&node.content[start..start + count]);
        Ok(count)
    }

    fn file_size(&mut self, path: &str) -> Result<u64, FsError> {
        let node = self.find_node(path)?;
        if node.is_dir {
            return Err(FsError::NotAFile);
        }
        Ok(node.content.len() as u64)
    }

    fn write_file(&mut self, path: &str, content: &[u8]) -> Result<(), FsError> {
        let (parent_path, file_name) = self.split_parent_and_name(path)?;
        let id = self.alloc_id();
        let parent_node = self.find_node_mut(parent_path)?;
        if !parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }

        if let Some(existing) = parent_node.children.iter_mut().find(|c| c.name == file_name) {
            if existing.is_dir {
                return Err(FsError::NotAFile);
            }
            existing.content = content.to_vec();
        } else {
            parent_node.children.push(MemNode {
                name: String::from(file_name),
                is_dir: false,
                content: content.to_vec(),
                children: Vec::new(),
                id,
            });
        }
        Ok(())
    }

    fn create_dir(&mut self, path: &str) -> Result<(), FsError> {
        let (parent_path, dir_name) = self.split_parent_and_name(path)?;
        let id = self.alloc_id();
        let parent_node = self.find_node_mut(parent_path)?;
        if !parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }

        if parent_node.children.iter().any(|c| c.name == dir_name) {
            return Err(FsError::AlreadyExists);
        }

        parent_node.children.push(MemNode {
            name: String::from(dir_name),
            is_dir: true,
            content: Vec::new(),
            children: Vec::new(),
            id,
        });
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), FsError> {
        let (parent_path, item_name) = self.split_parent_and_name(path)?;
        let parent_node = self.find_node_mut(parent_path)?;
        if !parent_node.is_dir {
            return Err(FsError::NotADirectory);
        }

        let pos = parent_node.children.iter().position(|c| c.name == item_name);
        match pos {
            Some(idx) => {
                parent_node.children.remove(idx);
                Ok(())
            }
            None => Err(FsError::NotFound),
        }
    }

    fn file_stat(&mut self, path: &str) -> Result<FileStat, FsError> {
        let node = self.find_node(path)?;
        Ok(FileStat {
            size: node.content.len() as u64,
            is_dir: node.is_dir,
            readonly: false,
        })
    }
}
