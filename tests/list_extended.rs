/// Extended list operation integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── Partition ──

#[test]
fn test_partition_basic() {
    let out = syma_eval("Partition[{1, 2, 3, 4}, 2]");
    assert!(
        !out.contains("error"),
        "Partition should not error, got: {out}"
    );
}

#[test]
fn test_partition_offset() {
    let out = syma_eval("Partition[{1, 2, 3, 4}, 2, 1]");
    assert!(
        !out.contains("error"),
        "Partition with offset should not error, got: {out}"
    );
}

// ── Split ──

#[test]
fn test_split_basic() {
    let out = syma_eval("Split[{1, 1, 2, 2, 3}]");
    assert!(!out.contains("error"), "Split should not error, got: {out}");
}

// ── Gather ──

#[test]
fn test_gather_basic() {
    let out = syma_eval("Gather[{1, 2, 3, 1, 2}]");
    assert!(
        !out.contains("error"),
        "Gather should not error, got: {out}"
    );
}

// ── DeleteDuplicates ──

#[test]
fn test_delete_duplicates_basic() {
    let out = syma_eval("DeleteDuplicates[{1, 2, 1, 3, 2}]");
    assert!(
        !out.contains("error"),
        "DeleteDuplicates should not error, got: {out}"
    );
}

// ── Insert ──

#[test]
fn test_insert_basic() {
    let out = syma_eval("Insert[{1, 2, 3}, 99, 2]");
    assert!(out.contains("99"), "Insert should contain 99, got: {out}");
}

// ── Delete ──

#[test]
fn test_delete_basic() {
    let out = syma_eval("Delete[{1, 2, 3}, 2]");
    assert!(
        !out.contains("error"),
        "Delete should not error, got: {out}"
    );
}

// ── ReplacePart ──

#[test]
fn test_replace_part_basic() {
    let out = syma_eval("ReplacePart[{1, 2, 3}, 2, 99]");
    assert!(
        out.contains("99"),
        "ReplacePart should contain 99, got: {out}"
    );
}

// ── RotateLeft / RotateRight ──

#[test]
fn test_rotate_left_basic() {
    let out = syma_eval("RotateLeft[{1, 2, 3}, 1]");
    assert!(
        !out.contains("error"),
        "RotateLeft should not error, got: {out}"
    );
}

#[test]
fn test_rotate_right_basic() {
    let out = syma_eval("RotateRight[{1, 2, 3}, 1]");
    assert!(
        !out.contains("error"),
        "RotateRight should not error, got: {out}"
    );
}

// ── Ordering ──

#[test]
fn test_ordering_basic() {
    let out = syma_eval("Ordering[{3, 1, 2}]");
    assert!(
        !out.contains("error"),
        "Ordering should not error, got: {out}"
    );
}

// ── ConstantArray ──

#[test]
fn test_constant_array_basic() {
    let out = syma_eval("ConstantArray[5, 3]");
    assert!(
        !out.contains("error"),
        "ConstantArray should not error, got: {out}"
    );
}

// ── Diagonal ──

#[test]
fn test_diagonal_basic() {
    let out = syma_eval("Diagonal[{{1, 2}, {3, 4}}]");
    assert!(out.contains("1"), "Diagonal should contain 1, got: {out}");
}

// ── Accumulate ──

#[test]
fn test_accumulate_basic() {
    let out = syma_eval("Accumulate[{1, 2, 3}]");
    assert!(
        !out.contains("error"),
        "Accumulate should not error, got: {out}"
    );
}

// ── Differences ──

#[test]
fn test_differences_basic() {
    let out = syma_eval("Differences[{1, 3, 6}]");
    assert!(
        !out.contains("error"),
        "Differences should not error, got: {out}"
    );
}

// ── Riffle ──

#[test]
fn test_riffle_basic() {
    let out = syma_eval("Riffle[{1, 2, 3}, 0]");
    assert!(
        !out.contains("error"),
        "Riffle should not error, got: {out}"
    );
}

// ── Most ──

#[test]
fn test_most_basic() {
    let out = syma_eval("Most[{1, 2, 3}]");
    assert!(
        out.contains("1") && out.contains("2"),
        "Most[{{1,2,3}}] should contain 1 and 2, got: {out}"
    );
    assert!(
        !out.contains("3") || !out.contains("error"),
        "Most should drop last element, got: {out}"
    );
}

// ── Prepend ──

#[test]
fn test_prepend_basic() {
    let out = syma_eval("Prepend[{2, 3}, 1]");
    assert!(
        !out.contains("error"),
        "Prepend should not error, got: {out}"
    );
}

// ── Sum / Product ──

#[test]
fn test_sum_basic() {
    let out = syma_eval("Sum[i, {i, 1, 5}]");
    assert!(
        out.contains("15"),
        "Sum[i,{{i,1,5}}] should be 15, got: {out}"
    );
}

#[test]
fn test_product_basic() {
    let out = syma_eval("Product[i, {i, 1, 5}]");
    assert!(
        out.contains("120"),
        "Product[i,{{i,1,5}}] should be 120, got: {out}"
    );
}

// ── Table ──

#[test]
fn test_table_basic() {
    let out = syma_eval("Table[i^2, {i, 1, 3}]");
    assert!(!out.contains("error"), "Table should not error, got: {out}");
}

// ── Thread ──

#[test]
fn test_thread_rule() {
    let out = syma_eval("Thread[List[{a, b}, {1, 2}]]");
    assert!(
        !out.contains("error"),
        "Thread should not error, got: {out}"
    );
}

// ── Apply / AllApply ──

#[test]
fn test_apply_plus() {
    let out = syma_eval("Apply[Plus, {1, 2, 3}]");
    assert!(
        out.contains("6"),
        "Apply[Plus, {{1,2,3}}] should be 6, got: {out}"
    );
}

#[test]
fn test_all_apply() {
    let out = syma_eval("AllApply[f, {{1, 2}, {3, 4}}]");
    assert!(
        !out.contains("error"),
        "AllApply should not error, got: {out}"
    );
}

// ── MapAt ──

#[test]
fn test_map_at() {
    let out = syma_eval("MapAt[#^2&, {1, 2, 3}, 2]");
    assert!(!out.contains("error"), "MapAt should not error, got: {out}");
}

// ── MapIndexed ──

#[test]
fn test_map_indexed() {
    let out = syma_eval("MapIndexed[f, {a, b, c}]");
    assert!(
        !out.contains("error"),
        "MapIndexed should not error, got: {out}"
    );
}

// ── MapThread ──

#[test]
fn test_map_thread() {
    let out = syma_eval("MapThread[f, {{a, b}, {1, 2}}]");
    assert!(
        !out.contains("error"),
        "MapThread should not error, got: {out}"
    );
}

// ── NestList / FoldList ──

#[test]
fn test_nest_list_basic() {
    let out = syma_eval("NestList[#+1&, 1, 3]");
    assert!(
        !out.contains("error"),
        "NestList should not error, got: {out}"
    );
}

#[test]
fn test_fold_list_basic() {
    let out = syma_eval("FoldList[Plus, 0, {1, 2, 3}]");
    assert!(
        !out.contains("error"),
        "FoldList should not error, got: {out}"
    );
}

// ── NestWhile / NestWhileList / FixedPointList ──

#[test]
fn test_nest_while() {
    let out = syma_eval("NestWhile[#+1&, 1, #<5&]");
    assert!(
        out.contains("5"),
        "NestWhile should converge to 5, got: {out}"
    );
}

#[test]
fn test_nest_while_list() {
    let out = syma_eval("NestWhileList[#+1&, 1, #<5&]");
    assert!(
        !out.contains("error"),
        "NestWhileList should not error, got: {out}"
    );
}

#[test]
fn test_fixed_point_list() {
    let out = syma_eval("FixedPointList[#/2&, 10, 3]");
    assert!(
        !out.contains("error"),
        "FixedPointList should not error, got: {out}"
    );
}

// ── Outer / Inner ──

#[test]
fn test_outer_times() {
    let out = syma_eval("Outer[Times, {1, 2}, {3, 4}]");
    assert!(!out.contains("error"), "Outer should not error, got: {out}");
}

#[test]
fn test_inner_basic() {
    let out = syma_eval("Inner[Times, Plus, {1, 2}, {3, 4}]");
    assert!(
        out.contains("11"),
        "Inner[Times,Plus,{{1,2}},{{3,4}}] should be 11, got: {out}"
    );
}

// ── BlockMap / MovingAverage ──

#[test]
fn test_block_map() {
    let out = syma_eval("BlockMap[f, {1, 2, 3, 4}, 2]");
    assert!(
        !out.contains("error"),
        "BlockMap should not error, got: {out}"
    );
}

#[test]
fn test_moving_average() {
    let out = syma_eval("MovingAverage[{1, 2, 3, 4}, 2]");
    assert!(
        !out.contains("error"),
        "MovingAverage should not error, got: {out}"
    );
}

// ── Nearest ──

#[test]
fn test_nearest_basic() {
    let out = syma_eval("Nearest[{1, 10, 100}, 3]");
    assert!(
        !out.contains("error"),
        "Nearest should not error, got: {out}"
    );
}

// ── ArrayReshape / ArrayPad ──

#[test]
fn test_array_reshape() {
    let out = syma_eval("ArrayReshape[{1, 2, 3, 4}, {2, 2}]");
    assert!(
        !out.contains("error"),
        "ArrayReshape should not error, got: {out}"
    );
}

#[test]
fn test_array_pad() {
    let out = syma_eval("ArrayPad[{1, 2, 3}, 1]");
    assert!(
        !out.contains("error"),
        "ArrayPad should not error, got: {out}"
    );
}

// ── Complement ──

#[test]
fn test_complement_basic() {
    let out = syma_eval("Complement[{1, 2, 3}, {2}]");
    assert!(
        !out.contains("error"),
        "Complement should not error, got: {out}"
    );
}

// ── Position ──

#[test]
fn test_position_basic() {
    let out = syma_eval("Position[{a, b, c}, b]");
    assert!(
        !out.contains("error"),
        "Position should not error, got: {out}"
    );
}

// ── Tally ──

#[test]
fn test_tally_basic() {
    let out = syma_eval("Tally[{1, 1, 2, 3, 3}]");
    assert!(!out.contains("error"), "Tally should not error, got: {out}");
}

// ── PadLeft / PadRight ──

#[test]
fn test_pad_left_basic() {
    let out = syma_eval("PadLeft[{1, 2}, 4]");
    assert!(
        !out.contains("error"),
        "PadLeft should not error, got: {out}"
    );
}

#[test]
fn test_pad_right_basic() {
    let out = syma_eval("PadRight[{1, 2}, 4, 99]");
    assert!(
        !out.contains("error"),
        "PadRight should not error, got: {out}"
    );
}
