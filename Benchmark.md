# 纯运算性能测分流程说明（Rust / JavaScript）

## 1. 测试目标

本测试用于**对比不同语言 / 运行时的“真实纯运算能力”**，刻意规避以下因素：

* 插件系统、IPC、序列化
* IO、系统调用
* BigInt / GC / Promise / async 等语言特性差异
* 外部库实现质量差异

**核心目标**：

> 在尽可能一致的算法与输入条件下，比较 CPU 对整数运算与浮点运算的原始吞吐能力。

---

## 2. 总体设计原则

### 2.1 算法一致

* Rust 与 JavaScript 使用**完全一致的算法结构**
* PRNG、循环结构、数学操作逐条对齐

### 2.2 只使用语言内建能力

* Rust：仅使用 `std`，无第三方 crate
* JavaScript：ECMAScript 2023 内建对象（Number / Math / ArrayBuffer / DataView）

### 2.3 不使用 BigInt（关键）

* JS BigInt 会严重影响整数性能
* 本测试统一采用 **32-bit 算法路径**
* 以此测试“真实算力”而非“语言特性性能”

### 2.4 防止空跑 / 编译器消除

* 所有测试均产生 **确定性 digest**
* Rust 使用 `black_box`
* JS 强制输出最终值

---

## 3. PRNG 设计（输入生成）

### 使用算法：`xorshift32`

原因：

* 极简、跨语言容易实现
* 全 32-bit 运算，JS 与 Rust 行为一致
* 性能开销极低，不成为 benchmark 主瓶颈

```text
x ^= x << 13
x ^= x >> 17
x ^= x << 5
```

### 特性

* 周期：2³² − 1
* 非加密
* 仅用于生成可复现伪随机输入

---

## 4. 测试项说明

### T1：INT32 混洗归约（T1_INT32_MIX）

#### 测试目的

评估：

* 32-bit 整数 ALU 吞吐
* 位运算、乘法、加法
* 分支预测
* 紧密循环性能

#### 算法结构

```text
for i in 0..N:
  x = rng.next_u32()
  v = x XOR acc
  v = rotate_left(v, i % 32)
  v = v * 常数
  v = v XOR (v >> 16)
  acc += v
  if v & 0x8000 != 0:
      acc ^= 常数
```

#### 输出

* `acc`（u32）
* 作为该测试的 digest

---

### T2：FP64 流式点积（T2_FP64_DOT）

#### 测试目的

评估：

* 双精度浮点乘加吞吐
* FPU / SIMD / pipeline 效率
* 浮点累积稳定性

#### 算法结构

```text
for i in 0..N:
  a = rng.next_f64_01()
  b = rng.next_f64_01()
  sum += a * b + 常量
```

#### 输出

* Rust：`sum.to_bits()`（u64）
* JS：IEEE754 位模式拆分后 XOR 成 u32
* 作为该测试的 digest

---

## 5. 执行流程（Run Protocol）

### 5.1 参数

* `seed`：PRNG 初始种子（默认 12345）
* `n1`：INT 测试迭代次数
* `n2`：FP 测试迭代次数
* `warmup`：热身轮数（默认 3）
* `repeats`：正式测试轮数（默认 9）

### 5.2 流程

```text
for each test:
  run warmup times (不计时)
  run repeats times:
    记录单次耗时
```

### 5.3 统计方式

对 `repeats` 次结果：

* `min`
* `p50`（中位数，主指标）
* `p95`
* `max`

---

## 6. 输出格式

### 示例（JSON）

```json
{
  "lang": "rust",
  "seed": 12345,
  "params": {
    "n1": 300000000,
    "n2": 200000000,
    "warmup": 3,
    "repeats": 9
  },
  "results": [
    {
      "id": "T1_INT32_MIX",
      "digest": "...",
      "time_ms": {
        "min": 1023.4,
        "p50": 1031.2,
        "p95": 1044.7,
        "max": 1051.9
      }
    }
  ]
}
```

---

## 7. 如何解读结果

### 7.1 看哪个指标？

* **首选 `p50`**：代表稳定性能
* `p95`：反映抖动与调度影响
* `min/max`：仅作参考

### 7.2 Rust vs JS 的合理预期

* T1（INT）：Rust 通常领先明显（位运算 + 无边界检查）
* T2（FP）：差距通常小于 INT（JS Number 是硬件双精度）

### 7.3 不要比较什么

* 不要跨不同机器比较绝对时间
* 不要用 debug / 非优化构建
* 不要混入 IO、Promise、BigInt

---

## 8. 已知限制

* JS 使用 `Date.now()` 计时，精度低于 `performance.now`
* JS 单线程，无法体现多核算力
* FP 结果受 IEEE754 舍入细节影响（但 digest 保证可复现）

---
