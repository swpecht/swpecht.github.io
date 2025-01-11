use crate::gamestate::SimState;

macro_rules! access_test {
    (  $x: expr, $y: expr, $c: expr, $item: tt ) => {{
        if $x.$item != $y.$item {
            $c.push(_DiffItem {
                name: stringify!($item).to_string(),
                l: format!("{:?}", $x.$item),
                r: format!("{:?}", $y.$item),
            })
        }
    }};
}

#[derive(Debug)]
#[allow(dead_code)]
pub(super) struct _DiffItem {
    pub name: String,
    pub l: String,
    pub r: String,
}

impl SimState {
    pub(super) fn _diff_between(&self, other: &SimState) -> Vec<_DiffItem> {
        let mut differences = Vec::new();

        // todo: add support for more granular vector comparison if needed
        access_test!(self, other, differences, generation);
        access_test!(self, other, differences, next_model_id);
        access_test!(self, other, differences, next_unit_id);
        access_test!(self, other, differences, queued_results);
        access_test!(self, other, differences, applied_results);
        access_test!(self, other, differences, initiative);
        access_test!(self, other, differences, locations);
        access_test!(self, other, differences, phase);
        access_test!(self, other, differences, is_start_of_turn);
        access_test!(self, other, differences, pending_chance_action);

        if self.models != other.models {
            for (l, r) in self.models.iter().zip(other.models.iter()) {
                if l != r {
                    differences.push(_DiffItem {
                        name: "Model difference".to_string(),
                        l: format!("{:?}", l),
                        r: format!("{:?}", r),
                    })
                }
            }
        }

        differences
    }
}
