//! Nix Archive (NAR) serializer and recursive hashing over an [`FsNode`] tree (pure: the tree is
//! read through [`PathIo::read_tree`](crate::PathIo::read_tree)).

use sha2::{Digest, Sha256};

use crate::caps::FsNode;

fn write_padded(out: &mut Vec<u8>, data: &[u8]) {
  let len = data.len() as u64;
  out.extend_from_slice(&len.to_le_bytes());
  out.extend_from_slice(data);
  let pad = (8 - (data.len() % 8)) % 8;
  out.extend(std::iter::repeat_n(0u8, pad));
}

fn write_str(out: &mut Vec<u8>, s: &str) {
  write_padded(out, s.as_bytes());
}

fn serialize_node(node: &FsNode, out: &mut Vec<u8>) {
  write_str(out, "(");
  write_str(out, "type");
  match node {
    FsNode::Symlink { target } => {
      write_str(out, "symlink");
      write_str(out, "target");
      write_str(out, target);
    }
    FsNode::Directory(entries) => {
      write_str(out, "directory");
      // `BTreeMap` iterates in byte order of the names, as Nix sorts them.
      for (name, child) in entries {
        write_str(out, "entry");
        write_str(out, "(");
        write_str(out, "name");
        write_str(out, name);
        write_str(out, "node");
        serialize_node(child, out);
        write_str(out, ")");
      }
    }
    FsNode::Regular {
      executable,
      contents,
    } => {
      write_str(out, "regular");
      if *executable {
        write_str(out, "executable");
        write_str(out, "");
      }
      write_str(out, "contents");
      write_padded(out, contents);
    }
  }
  write_str(out, ")");
}

/// Serializes `node` as a Nix Archive into `out`.
pub fn nar_serialize(node: &FsNode, out: &mut Vec<u8>) {
  write_str(out, "nix-archive-1");
  serialize_node(node, out);
}

/// The NAR bytes of `node`.
pub fn nar_bytes(node: &FsNode) -> Vec<u8> {
  let mut out = Vec::new();
  nar_serialize(node, &mut out);
  out
}

/// SHA256 of the NAR of `node` (Nix `outputHashMode = "recursive"`).
pub fn hash_nar(node: &FsNode) -> [u8; 32] {
  Sha256::digest(nar_bytes(node)).into()
}

#[cfg(test)]
pub(crate) mod tests {
  use std::collections::BTreeMap;

  use super::*;
  use crate::hash_parse::format_nix32;
  use crate::ingest::hash_flat_bytes;

  pub(crate) fn file(contents: &[u8]) -> FsNode {
    FsNode::Regular {
      executable: false,
      contents: contents.to_vec(),
    }
  }

  /// `fixtures/nar/tree` in memory: `a/x`, `b/y`, `link -> ../a/x`.
  pub(crate) fn fixture_tree() -> FsNode {
    let dir = |entries: Vec<(&str, FsNode)>| {
      FsNode::Directory(
        entries
          .into_iter()
          .map(|(n, c)| (n.to_owned(), c))
          .collect::<BTreeMap<_, _>>(),
      )
    };
    dir(vec![
      ("a", dir(vec![("x", file(b"one"))])),
      ("b", dir(vec![("y", file(b"two"))])),
      (
        "link",
        FsNode::Symlink {
          target: "../a/x".into(),
        },
      ),
    ])
  }

  #[test]
  fn golden_recursive_matches_nix_hash() {
    let dig = hash_nar(&fixture_tree());
    assert_eq!(
      hex::encode(dig),
      "9d1ecf78f5e63c166734d9c5f2f4faf9fe3b116adc3dfa0721e1f2fd38120234"
    );
    assert_eq!(
      format_nix32(&dig),
      "0d0228wgvwp1443zlgfwd88kpzprzbsg5ifr6ikicg76ymwcy7lx"
    );
  }

  #[test]
  fn nar_of_a_file_differs_from_its_flat_hash() {
    assert_ne!(hash_nar(&file(b"hello")), hash_flat_bytes(b"hello"));
  }

  #[test]
  fn executable_bit_and_empty_dir_change_the_nar() {
    let plain = file(b"#!/bin/sh\n");
    let exec = FsNode::Regular {
      executable: true,
      contents: b"#!/bin/sh\n".to_vec(),
    };
    assert_ne!(hash_nar(&plain), hash_nar(&exec));
    let empty = nar_bytes(&FsNode::Directory(BTreeMap::new()));
    assert!(!empty.is_empty());
    assert_eq!(empty.len() % 8, 0, "NAR strings are 8-byte padded");
  }
}
