use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use fantastic_pool::msg::{
    CalcMintResult, CalcRedeemResult, ExecuteMsg, GetPriceResult, InstantiateMsg, PoolInfoResponse,
    QueryMsg,
};
use fantastic_pool::pool::{PoolConfig, UserInfo};
use std::env::current_dir;
use std::fs::create_dir_all;
fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(UserInfo), &out_dir);
    export_schema(&schema_for!(PoolConfig), &out_dir);
    export_schema(&schema_for!(PoolInfoResponse), &out_dir);
    export_schema(&schema_for!(GetPriceResult), &out_dir);
    export_schema(&schema_for!(CalcMintResult), &out_dir);
    export_schema(&schema_for!(CalcRedeemResult), &out_dir);
}
