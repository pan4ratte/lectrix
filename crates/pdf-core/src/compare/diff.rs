//! Lines up two sequences of words (section 6.13): which word of one is which word of the
//! other. Words that occur once in both anchor the alignment (patience diff), which keeps
//! it right across moved and rewritten passages and fast on long documents; between
//! anchors, and where nothing is unique, the shortest edit script (Myers) decides, within
//! a budget past which the stretch counts as changed as a whole.

use std::collections::HashMap;

/// Work the Myers fallback may do on one stretch: its length (both sides) times the edit
/// distance it tries. Past it, the stretch has no matches.
const MYERS_BUDGET: usize = 40_000_000;
/// The longest edit script the fallback looks for.
const MYERS_MAX_D: usize = 2_000;

/// The pairs `(i, j)` with `a[i] == b[j]` that line the two up, increasing in both.
/// `cancelled` is asked now and then; once it says so, what was found so far is returned.
pub fn matches(a: &[u32], b: &[u32], cancelled: &dyn Fn() -> bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut work = vec![(0, a.len(), 0, b.len())];
    let mut rounds = 0u32;
    while let Some((mut al, mut ah, mut bl, mut bh)) = work.pop() {
        rounds = rounds.wrapping_add(1);
        if rounds.is_multiple_of(256) && cancelled() {
            break;
        }
        while al < ah && bl < bh && a[al] == b[bl] {
            out.push((al, bl));
            al += 1;
            bl += 1;
        }
        while al < ah && bl < bh && a[ah - 1] == b[bh - 1] {
            ah -= 1;
            bh -= 1;
            out.push((ah, bh));
        }
        if al == ah || bl == bh {
            continue;
        }
        let mut anchors = anchors(&a[al..ah], &b[bl..bh], 1);
        if anchors.is_empty() {
            // Nothing unique (a table of numbers, a list of the same few words): words
            // that occur as often in both, a few times each, anchor by their order.
            anchors = self::anchors(&a[al..ah], &b[bl..bh], REPEATED_MAX);
        }
        if anchors.is_empty() {
            if let Some(found) = myers(&a[al..ah], &b[bl..bh]) {
                out.extend(found.into_iter().map(|(i, j)| (i + al, j + bl)));
            }
            continue;
        }
        let (mut pa, mut pb) = (al, bl);
        for (i, j) in anchors {
            let (i, j) = (i + al, j + bl);
            work.push((pa, i, pb, j));
            out.push((i, j));
            pa = i + 1;
            pb = j + 1;
        }
        work.push((pa, ah, pb, bh));
    }
    out.sort_unstable();
    out
}

/// The most times a word may occur (as often in both) to anchor a stretch with no unique
/// words.
const REPEATED_MAX: usize = 64;

/// The words that occur as often in `a` as in `b`, at most `most` times, each occurrence
/// paired with the same occurrence in the other (with `most` 1, the words unique in both),
/// the longest run of these pairs that is in the same order in both (patience sorting).
fn anchors(a: &[u32], b: &[u32], most: usize) -> Vec<(usize, usize)> {
    // Word to its places in a and in b (up to one more than `most`, to know it is more).
    let mut seen: HashMap<u32, (Vec<usize>, Vec<usize>)> = HashMap::with_capacity(a.len());
    for (i, w) in a.iter().enumerate() {
        let e = seen.entry(*w).or_default();
        if e.0.len() <= most {
            e.0.push(i);
        }
    }
    for (j, w) in b.iter().enumerate() {
        if let Some(e) = seen.get_mut(w)
            && e.1.len() <= most
        {
            e.1.push(j);
        }
    }
    let mut pairs: Vec<(usize, usize)> = seen
        .values()
        .filter(|(pa, pb)| pa.len() == pb.len() && pa.len() <= most)
        .flat_map(|(pa, pb)| pa.iter().copied().zip(pb.iter().copied()))
        .collect();
    pairs.sort_unstable();
    longest_increasing(&pairs)
}

/// The longest subsequence of `pairs` (sorted by their first) increasing in the second.
fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // tails[k]: the pair ending the best run of length k + 1 seen so far.
    let mut tails: Vec<usize> = Vec::new();
    let mut back: Vec<Option<usize>> = vec![None; pairs.len()];
    for (n, &(_, j)) in pairs.iter().enumerate() {
        let k = tails.partition_point(|&t| pairs[t].1 < j);
        if k > 0 {
            back[n] = Some(tails[k - 1]);
        }
        if k == tails.len() {
            tails.push(n);
        } else {
            tails[k] = n;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(n) = at {
        out.push(pairs[n]);
        at = back[n];
    }
    out.reverse();
    out
}

/// The matches of the shortest edit script between `a` and `b` (Myers' O(ND) algorithm),
/// or None when it is longer than the budget allows.
fn myers(a: &[u32], b: &[u32]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let limit = (MYERS_BUDGET / (a.len() + b.len()).max(1))
        .min(MYERS_MAX_D)
        .min(a.len() + b.len()) as isize;
    let off = limit + 1;
    let mut v = vec![0isize; (2 * limit + 3) as usize];
    // trace[d]: v for diagonals -(d + 1) ..= d + 1 as round d began.
    let mut trace: Vec<Vec<isize>> = Vec::new();
    let mut end = None;
    'rounds: for d in 0..=limit {
        trace.push(v[(off - d - 1) as usize..=(off + d + 1) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let at = |k: isize| v[(off + k) as usize];
            let mut x = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
                at(k + 1)
            } else {
                at(k - 1) + 1
            };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(off + k) as usize] = x;
            if x >= n && y >= m {
                end = Some(d);
                break 'rounds;
            }
            k += 2;
        }
    }
    let last = end?;
    let mut out = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..=last).rev() {
        let row = &trace[d as usize];
        let at = |k: isize| row[(k + d + 1) as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = at(prev_k);
        let prev_y = prev_x - prev_k;
        while x > prev_x && y > prev_y && x > 0 && y > 0 {
            x -= 1;
            y -= 1;
            out.push((x as usize, y as usize));
        }
        if d > 0 {
            x = prev_x;
            y = prev_y;
        }
    }
    out.reverse();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(s: &str) -> Vec<u32> {
        s.split_whitespace()
            .map(|w| {
                w.bytes()
                    .fold(7u32, |h, c| h.wrapping_mul(31).wrapping_add(c.into()))
            })
            .collect()
    }

    fn lcs_len(a: &[u32], b: &[u32]) -> usize {
        let mut row = vec![0usize; b.len() + 1];
        for x in a {
            let mut diag = 0;
            for (j, y) in b.iter().enumerate() {
                let up = row[j + 1];
                row[j + 1] = if x == y { diag + 1 } else { up.max(row[j]) };
                diag = up;
            }
        }
        row[b.len()]
    }

    fn check(a: &[u32], b: &[u32], found: &[(usize, usize)]) {
        for w in found.windows(2) {
            assert!(
                w[0].0 < w[1].0 && w[0].1 < w[1].1,
                "not increasing: {found:?}"
            );
        }
        for &(i, j) in found {
            assert_eq!(a[i], b[j]);
        }
    }

    #[test]
    fn myers_finds_the_longest_common_subsequence() {
        let cases = [
            ("a b c a b b a", "c b a b a c"),
            ("the cat sat on the mat", "the dog sat on a mat"),
            ("x y z", "p q r"),
            ("", "a b"),
            ("a a a a", "a a"),
        ];
        for (a, b) in cases {
            let (a, b) = (ids(a), ids(b));
            let found = myers(&a, &b).unwrap();
            check(&a, &b, &found);
            assert_eq!(found.len(), lcs_len(&a, &b));
        }
    }

    #[test]
    fn lines_up_a_paragraph_moved_and_words_changed() {
        let a = ids("one two three four five six seven eight nine ten the the a a");
        let b = ids("one two 3 four five six seven nine ten eleven the a the a");
        let found = matches(&a, &b, &|| false);
        check(&a, &b, &found);
        assert_eq!(found.len(), lcs_len(&a, &b));
    }

    #[test]
    fn stays_fast_on_long_texts() {
        // 200 000 words with an edit every thousand.
        let words: Vec<u32> = (0..200_000u32).map(|i| i % 5_000).collect();
        let mut other = words.clone();
        for i in (0..other.len()).step_by(1_000) {
            other[i] = 999_999;
        }
        let start = std::time::Instant::now();
        let found = matches(&words, &other, &|| false);
        check(&words, &other, &found);
        assert!(found.len() >= 199_000, "{}", found.len());
        assert!(start.elapsed().as_secs_f64() < 5.0, "{:?}", start.elapsed());
    }
}
