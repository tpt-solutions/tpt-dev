(module
  (global $g (mut i32) (i32.const 0))
  (func (export "add") (param i32 i32) (result i32) local.get 0 local.get 1 i32.add)
  (func (export "div_s") (param i32 i32) (result i32) local.get 0 local.get 1 i32.div_s)
  (func (export "div_u") (param i32 i32) (result i32) local.get 0 local.get 1 i32.div_u)
  (func (export "rem_s") (param i32 i32) (result i32) local.get 0 local.get 1 i32.rem_s)
  (func (export "shl") (param i32 i32) (result i32) local.get 0 local.get 1 i32.shl)
  (func (export "rotl") (param i32 i32) (result i32) local.get 0 local.get 1 i32.rotl)
  (func (export "lt_u") (param i32 i32) (result i32) local.get 0 local.get 1 i32.lt_u)
  (func (export "clz") (param i32) (result i32) local.get 0 i32.clz)
  (func (export "neg") (param i32) (result i32) i32.const 0 local.get 0 i32.sub)
  (func $fact (export "fact") (param i64) (result i64)
    local.get 0 i64.eqz
    if (result i64)
      i64.const 1
    else
      local.get 0
      local.get 0 i64.const 1 i64.sub
      call $fact
      i64.mul
    end)
  (func (export "sum") (param i32) (result i32) (local i32)
    block
      loop
        local.get 0 i32.eqz br_if 1
        local.get 1 local.get 0 i32.add local.set 1
        local.get 0 i32.const 1 i32.sub local.set 0
        br 0
      end
    end
    local.get 1)
  (func (export "pick") (param i32 i32 i32) (result i32)
    local.get 1 local.get 2 local.get 0 select)
  (func (export "sign") (param i32) (result i32)
    local.get 0 i32.const 0 i32.lt_s
    if (result i32)
      i32.const -1
    else
      local.get 0 i32.eqz
      if (result i32)
        i32.const 0
      else
        i32.const 1
      end
    end)
  (func (export "tee") (param i32) (result i32) (local i32)
    local.get 0 i32.const 5 i32.add local.tee 1 drop local.get 1)
  (func (export "early") (param i32) (result i32)
    local.get 0
    if
      i32.const 100
      return
    end
    i32.const 7)
  (func (export "block_result") (param i32) (result i32)
    block (result i32)
      i32.const 9
      local.get 0
      br_if 0
      drop
      i32.const 3
    end)
  (func (export "i64ops") (param i64 i64) (result i64)
    local.get 0 local.get 1 i64.mul i64.const 3 i64.rem_u)
  (func (export "wrap") (param i64) (result i32) local.get 0 i32.wrap_i64)
  (func (export "ext_u") (param i32) (result i64) local.get 0 i64.extend_i32_u)
  (func (export "ext_s") (param i32) (result i64) local.get 0 i64.extend_i32_s)
  (func (export "bump") (result i32)
    global.get $g i32.const 1 i32.add global.set $g global.get $g)
  (func (export "trap") unreachable)
  (func (export "divz") (param i32) (result i32) i32.const 1 local.get 0 i32.div_s)
  (func (export "spin") loop br 0 end)
  (func $down (export "deep") (param i32) (result i32) local.get 0 call $down)
)
