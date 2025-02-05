use std::collections::BTreeSet;

pub(crate) fn ordered_3tuple((u, v, w): (u32, u32, u32)) -> (u32, u32, u32) {
    let mut arr = [u, v, w];
    arr.sort();
    (arr[0], arr[1], arr[2])
}

pub(crate) fn ordered_2tuple(u: u32, v: u32) -> (u32, u32) {
    if u > v {
        (u, v)
    } else {
        (v, u)
    }
}

/// arguments:
/// - vertices: The vec of vertices
/// - indices: A vec of indices, indexing into the vertices
///
/// Returns the values in indices, sorted such that the corresponding points in vertices
/// are ordered in a clockwise fashion when viewed looking onto the sphere from the outside.
pub(crate) fn sort_poly_vertices(vertices: &Vec<[f32; 3]>, indices: Vec<u32>) -> Vec<u32> {
    let mut u = indices[0];
    let mut seen = BTreeSet::from([u]);
    let mut sorted = vec![u];

    // Get the indices closest to i and pick one that isnt already in sorted
    loop {
        if seen.len() == indices.len() {
            break;
        }

        let mut max_distance = f32::INFINITY;
        let mut j = usize::MAX;
        for (i, v) in indices.clone().into_iter().enumerate() {
            // i is the much smaller index
            // v is the vertex-index
            if v == u {
                continue;
            }

            let a = vertices[u as usize];
            let b = vertices[v as usize];

            let distance = (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);

            if distance < max_distance && !seen.contains(&v) {
                max_distance = distance;
                j = i;
            }
        }

        u = indices[j];
        seen.insert(u);
        sorted.push(u);
    }

    sorted
}
