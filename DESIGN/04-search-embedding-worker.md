# 检索、Embedding、Worker

检索会把 exact text/key scoring 和向量检索结合起来；向量检索只在存在 embedded
records 时运行。worker 负责把 pending records 转换为 embedded records。

## Exact Search

`exact_results` 会构建一个 lowercase search document，包含：

- keys 两次，用于提高 key 权重。
- content。
- summary。
- source ref。
- memory type。
- tags。

Tokenization 使用 regex `[\w./:-]+`，并把 tokens 转为小写。

对每条 record：

1. Token overlap 为 `overlap / query_token_count`。
2. 如果 lowercased full query 至少 4 个字符且出现在 haystack 中，substring bonus 为
   `0.2`。
3. keys 命中时，key bonus 为 `0.25 + min(matched_key_count, 3) * 0.05`。
4. relevance 为 `base * 0.65 + substring_bonus + key_bonus`，上限 `1.0`。
5. relevance 为 0 的结果被丢弃。
6. keys 命中时 match reason 是 `direct_keys`，否则是 `direct_text`。
7. pending records 还会额外带上 `pending` match reason。

Matched keys 通过两种方式检测：query/key 字符串长度至少 3 时做 substring
containment，或检查 query tokens 和 key tokens 的 token overlap。

## Vector Search

`vector_results`：

1. 使用配置的 provider embed query。
2. 调用 active backend vector search。
3. 在已经过滤过的 record set 中查找 vector hit ids。
4. 直接把 vector distance/score 转成 relevance，并 clamp 到 `0..1`。
5. 使用 match reason `direct_embedding`。
6. 按最终 score 排序。

Backend vector search 只检索 embedded records：

```text
embedding_status == "embedded"
```

## Result Merging

`merge_search_results` 按 UUID 把 vector candidates 合并到 exact results：

- 已存在 result：保留 max relevance，并追加新的 match reasons。
- 新 result：直接追加。
- 最终列表按 score 降序排序。

## Reliability

Reliability 使用 source authority、调用方 confidence、conflict count 和
pending/failed penalty。

Source authority：

- `user`：0.95
- `official_docs`：0.9
- `repo`：0.85
- `tool_output`：0.75
- `agent_inferred`：0.45
- unknown：0.5

公式：

```text
reliability = clamp(
  0.52 * confidence
  + 0.35 * authority
  + 0.13
  - min(conflict_count * 0.12, 0.5)
  - pending_or_failed_penalty,
  0.0,
  1.0
)
```

对于 `pending` 或 `failed`，`pending_or_failed_penalty` 为 `0.05`。

## Final Score

Final score：

```text
score = relevance * 0.65 + reliability * 0.35
```

Score 和 relevance 都四舍五入到 4 位小数。

## Associative Search

Associative retrieval 只在以下条件同时满足时运行：

- `retrieval.associative.enabled` 为 true。
- `retrieval.associative.limit > 0`。
- direct results 生成了非空 seed。

当前实现：

1. direct results 先按 direct limit 截断。
2. 从 direct matched keys 和 keys 构建 seed，最多取 16 个字符串。
3. 排除已经 selected records。
4. 用 seed 对剩余 records 运行 exact search。
5. 把 match reasons 替换为 `associative_keys`。
6. 将 relevance 乘以 `0.92` 进行降权。
7. 重新计算 score。
8. 只保留 score 至少为 `associative.min_score` 的 records。
9. 截断到 `associative.limit`。

`associative.strategy` config value 会被保存，但目前不会切换不同算法。

## Search Command Flow

`search`：

1. 加载 runtime config。
2. 从 backend 读取 records。
3. 应用 memory type、scope 和 tag filters。
4. 计算 exact results。
5. 如果任一 filtered record 已 embedded，则尝试 vector search。
6. vector search 失败时添加 vector warning。
7. 将 direct results 截断到 `--limit` 或 `retrieval.default_limit`，最小为 1。
8. 可选添加 associative results。
9. 对返回的 ids 调用 `mark_accessed`。
10. 返回包含 `ok`、`query`、`warnings` 和 `results` 的 JSON。

## Marking Access

`mark_accessed`：

- 读取所有 records。
- 对返回 ids 的 records 递增 `access_count`。
- 将 `last_accessed_at` 设为当前 timestamp。
- 在没有新 vector 的情况下 upsert 每条 changed record，从而保留 existing vector。

## Embedding Providers

### `fake`

确定性本地 embedding：

- 按 whitespace 拆分文本。
- 用 SHA-256 hash lowercase tokens。
- 把每个 token 映射到一个 vector index。
- 根据 digest byte parity 添加 `+1` 或 `-1`。
- 归一化 vector length。

如果文本没有 whitespace tokens，则把全文作为一个 token。

### `ollama`

使用配置 endpoint，或默认 `http://localhost:11434`。

第一次请求：

- `POST <endpoint>/api/embed`
- payload：`{ "model": <model>, "input": <text> }`
- 预期响应：`embeddings[0]`

Fallback 请求：

- `POST <endpoint>/api/embeddings`
- payload：`{ "model": <model>, "prompt": <text> }`
- 预期响应：`embedding`

### `openai`

使用配置 endpoint，或 `https://api.openai.com/v1/embeddings`。

要求：

- 环境变量 `OPENAI_API_KEY`

请求：

- 使用 API key 做 bearer auth。
- payload：`{ "model": <model>, "input": <text> }`
- 预期响应：`data[0].embedding`

## Embedding Validation

所有 provider 都通过 `vector_from_json` 返回 vectors；它会拒绝：

- 非 number vector values。
- 长度和 configured dimension 不同的 vector。

## Worker

`cmd_worker` 执行一轮 pass：

1. 解析 runtime root 并加载 runtime config。
2. 确保 backend。
3. 获取 pending records；带 `--retry-failed` 时也获取 failed records。
4. 对每条 record：
   - 递增 processed count。
   - 将 status 设为 `embedding`。
   - 递增 attempts。
   - 清空 error。
   - 更新时间戳。
   - 在没有 vector 的情况下 upsert record。
   - 当 keys 存在时，embed `content` 加 `Search keys: ...`。
   - 成功时，将 status 设为 `embedded`，清空 error，更新时间戳，并 upsert vector。
   - 失败时，将 status 设为 `failed`，保存 error 前 500 个字符，更新时间戳，
     在没有 vector 的情况下 upsert，并添加 failure object。
5. 返回 processed/embedded/failed counts 和 failure details。

CLI 接受 `worker --once`，但当前 command dispatch 始终只运行一轮 worker pass。
