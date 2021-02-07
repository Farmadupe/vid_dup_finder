use std::{iter, path::PathBuf};

use super::TemporalHash;

fn empty_frames(num_frames: usize) -> Vec<Vec<u64>> {
    let empty_frame: Vec<u64> = vec![0];
    iter::repeat(&empty_frame).take(num_frames).cloned().collect()
}

fn full_frames(num_frames: usize) -> Vec<Vec<u64>> {
    let empty_frame: Vec<u64> = vec![u64::MAX];
    iter::repeat(&empty_frame).take(num_frames).cloned().collect()
}

fn empty_hash_components(num_frames: usize) -> (PathBuf, Vec<Vec<u64>>, Vec<Vec<u64>>) {
    let empty_s_hash = empty_frames(num_frames);
    let empty_t_hash = empty_frames(num_frames - 1);
    let path = PathBuf::from("");

    (path, empty_s_hash, empty_t_hash)
}

fn full_hash_components(num_frames: usize) -> (PathBuf, Vec<Vec<u64>>, Vec<Vec<u64>>) {
    let empty_s_hash = full_frames(num_frames);
    let empty_t_hash = full_frames(num_frames - 1);
    let path = PathBuf::from("");

    (path, empty_s_hash, empty_t_hash)
}

#[test]
//take two identical TemporalHashes. Demonstrate that their distance is 0.
fn test_distance_0() {
    let (p, s, t) = empty_hash_components(10);

    let thash_1 = TemporalHash::new(p, s, t).unwrap();
    let thash_2 = thash_1.clone();

    assert!(thash_1.distance(&thash_2) == 0);
    assert!(thash_2.distance(&thash_1) == 0);
}

#[test]
//introduce a single bit difference in the
fn test_distance_1_minframes_spatial() {
    let (p1, mut s1, t1) = empty_hash_components(2);
    s1[0][0] = 1;
    s1[1][0] = 1;
    let h1 = TemporalHash::new(p1, s1, t1).unwrap();

    let (p2, s2, t2) = empty_hash_components(2);
    let h2 = TemporalHash::new(p2, s2, t2).unwrap();

    let h1_h2 = h1.distance(&h2);
    let h2_h1 = h2.distance(&h1);

    assert!(h1_h2 == 320);
    assert!(h2_h1 == 320);
}

#[test]
//introduce a single bit difference in the
fn test_distance_1_maxframes_spatial() {
    let (p1, mut s1, t1) = empty_hash_components(10);
    s1[00][0] = 1;
    s1[01][0] = 1;
    s1[02][0] = 1;
    s1[03][0] = 1;
    s1[04][0] = 1;
    s1[05][0] = 1;
    s1[06][0] = 1;
    s1[07][0] = 1;
    s1[08][0] = 1;
    s1[09][0] = 1;
    let h1 = TemporalHash::new(p1, s1, t1).unwrap();

    let (p2, s2, t2) = empty_hash_components(10);
    let h2 = TemporalHash::new(p2, s2, t2).unwrap();

    let h1_h2 = h1.distance(&h2);
    let h2_h1 = h2.distance(&h1);

    assert!(h1_h2 == 320);
    assert!(h2_h1 == 320);
}

#[test]
//introduce a single bit difference in the
fn test_distance_1_minframes_temporal() {
    let (p1, s1, mut t1) = empty_hash_components(2);
    t1[0][0] = 1;
    let h1 = TemporalHash::new(p1, s1, t1).unwrap();

    let (p2, s2, t2) = empty_hash_components(2);
    let h2 = TemporalHash::new(p2, s2, t2).unwrap();

    let h1_h2 = h1.distance(&h2);
    let h2_h1 = h2.distance(&h1);

    assert!(h1_h2 == 320, "expected 320, got {}", h1_h2);
    assert!(h2_h1 == 320, "expected 320, got {}", h2_h1);
}

#[test]

//introduce a single bit difference in the
fn test_distance_1_maxframes_temporal() {
    let (p1, s1, mut t1) = empty_hash_components(10);
    t1[00][0] = 1;
    t1[01][0] = 1;
    t1[02][0] = 1;
    t1[03][0] = 1;
    t1[04][0] = 1;
    t1[05][0] = 1;
    t1[06][0] = 1;
    t1[07][0] = 1;
    t1[08][0] = 1;
    let h1 = TemporalHash::new(p1, s1, t1).unwrap();

    let (p2, s2, t2) = empty_hash_components(10);
    let h2 = TemporalHash::new(p2, s2, t2).unwrap();

    let h1_h2 = h1.distance(&h2);
    let h2_h1 = h2.distance(&h1);

    assert!(h1_h2 == 319, "expected 320, got {}", h1_h2);
    assert!(h2_h1 == 319, "expected 320, got {}", h2_h1);
}

//take two max-different TemporalHashes. Demonstrate that their distance is 1.
#[test]
fn test_maxdist_minframes() {
    let (p1, s1, t1) = empty_hash_components(2);
    let h1 = TemporalHash::new(p1, s1, t1).unwrap();

    let (p2, s2, t2) = full_hash_components(2);
    let h2 = TemporalHash::new(p2, s2, t2).unwrap();

    let h1_h2 = h1.distance(&h2);
    let h2_h1 = h2.distance(&h1);

    let expected = 40960;
    assert!(h1_h2 == expected, "expected {}, got {}", expected, h1_h2);
    assert!(h2_h1 == expected, "expected {}, got {}", expected, h2_h1);
}

//take two max-different TemporalHashes. Demonstrate that their distance is 1.
#[test]
fn test_maxdist_maxframes() {
    let (p1, s1, t1) = empty_hash_components(10);
    let h1 = TemporalHash::new(p1, s1, t1).unwrap();

    let (p2, s2, t2) = full_hash_components(10);
    let h2 = TemporalHash::new(p2, s2, t2).unwrap();

    let h1_h2 = h1.distance(&h2);
    let h2_h1 = h2.distance(&h1);

    let expected = 40928;
    assert!(h1_h2 == expected, "expected {}, got {}", expected, h1_h2);
    assert!(h2_h1 == expected, "expected {}, got {}", expected, h2_h1);
}
