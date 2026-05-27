from __future__ import annotations

import json
from typing import Any


def record_from_entity(entity: dict[str, Any]) -> dict[str, Any]:
    updated_at = str(entity.get("updated_at") or "")
    return {
        "uuid": str(entity.get("uuid") or entity.get("memory_id") or ""),
        "content": str(entity.get("content") or ""),
        "keys": string_list(entity.get("keys")),
        "summary": str(entity.get("summary") or ""),
        "embedding_status": str(entity.get("embedding_status") or "pending"),
        "embedding_error": optional_string(entity.get("embedding_error")),
        "embedding_attempts": int(entity.get("embedding_attempts") or 0),
        "memory_type": str(entity.get("memory_type") or "project"),
        "scope": str(entity.get("scope") or "project"),
        "root_path": str(entity.get("root_path") or ""),
        "tags": string_list(entity.get("tags")),
        "source_kind": str(entity.get("source_kind") or "agent_inferred"),
        "source_ref": str(entity.get("source_ref") or ""),
        "created_at": str(entity.get("created_at") or updated_at),
        "updated_at": updated_at,
        "last_accessed_at": optional_string(entity.get("last_accessed_at")),
        "access_count": int(entity.get("access_count") or 0),
        "conflict_count": int(entity.get("conflict_count") or 0),
        "confidence": float(entity.get("confidence") or 0.0),
        "verified_at": optional_string(entity.get("verified_at")),
        "stale_after_days": optional_int(entity.get("stale_after_days")),
        "embedding_provider": str(entity.get("embedding_provider") or ""),
        "embedding_model": str(entity.get("embedding_model") or ""),
        "embedding_dim": int(entity.get("embedding_dim") or 0),
        "schema_version": int(entity.get("schema_version") or 1),
    }


def optional_string(value: Any) -> str | None:
    if value is None or value == "":
        return None
    return str(value)


def optional_int(value: Any) -> int | None:
    if value is None:
        return None
    number = int(value)
    if number < 0:
        return None
    return number


def string_list(value: Any) -> list[str]:
    if isinstance(value, list):
        return [str(item) for item in value]
    if not isinstance(value, str) or value == "":
        return []
    try:
        decoded = json.loads(value)
        if isinstance(decoded, list):
            return [str(item) for item in decoded]
    except json.JSONDecodeError:
        pass
    return [item.strip() for item in value.split(",") if item.strip()]
