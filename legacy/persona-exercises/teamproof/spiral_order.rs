fn spiral_order(matrix: Vec<Vec<i32>>) -> Vec<i32> {
    let mut result = Vec::new();
    if matrix.is_empty() {
        return result;
    }

    let (mut top, mut bottom) = (0, matrix.len());
    let (mut left, mut right) = (0, matrix[0].len());

    while top < bottom && left < right {
        for i in left..right {
            result.push(matrix[top][i]);
        }
        top += 1;

        for i in top..bottom {
            result.push(matrix[i][right - 1]);
        }
        right -= 1;

        if top < bottom {
            for i in (left..right).rev() {
                result.push(matrix[bottom - 1][i]);
            }
            bottom -= 1;
        }

        if left < right {
            for i in (top..bottom).rev() {
                result.push(matrix[i][left]);
            }
            left += 1;
        }
    }

    result
}