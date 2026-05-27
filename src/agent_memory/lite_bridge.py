from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from agent_memory.lite_bridge_codec import record_from_entity

PRIMARY_FIELD = "uuid"
VECTOR_FIELD = "dense_vector"

RECORD_OUTPUT_FIELDS = [
    PRIMARY_FIELD,
    "content",
    "keys",
    "summary",
    "embedding_status",
    "embedding_error",
    "embedding_attempts",
    "memory_type",
    "scope",
    "root_path",
    "tags",
    "source_kind",
    "source_ref",
    "created_at",
    "updated_at",
    "last_accessed_at",
    "access_count",
    "conflict_count",
    "confidence",
    "verified_at",
    "stale_after_days",
    "embedding_provider",
    "embedding_model",
    "embedding_dim",
    "schema_version",
    "reliability",
]


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="agent_memory.lite_bridge")
    sub = parser.add_subparsers(dest="command", required=True)

    ensure = sub.add_parser("ensure")
    ensure.add_argument("--db", required=True)
    ensure.add_argument("--collection", required=True)
    ensure.add_argument("--dim", required=True, type=int)

    upsert = sub.add_parser("upsert")
    upsert.add_argument("--db", required=True)
    upsert.add_argument("--collection", required=True)
    upsert.add_argument("--dim", required=True, type=int)

    list_records = sub.add_parser("list")
    list_records.add_argument("--db")
    list_records.add_argument("--uri")
    list_records.add_argument("--collection", required=True)

    get = sub.add_parser("get")
    get.add_argument("--db")
    get.add_argument("--uri")
    get.add_argument("--collection", required=True)
    get.add_argument("--id", required=True)

    delete = sub.add_parser("delete")
    delete.add_argument("--db", required=True)
    delete.add_argument("--collection", required=True)
    delete.add_argument("--id", required=True)

    pending = sub.add_parser("pending")
    pending.add_argument("--db", required=True)
    pending.add_argument("--collection", required=True)
    pending.add_argument("--limit", required=True, type=int)
    pending.add_argument("--retry-failed", action="store_true")
    pending.add_argument("--no-retry-failed", action="store_true")

    search = sub.add_parser("search")
    search.add_argument("--db")
    search.add_argument("--uri")
    search.add_argument("--collection", required=True)
    search.add_argument("--limit", required=True, type=int)

    args = parser.parse_args(argv)
    try:
        if args.command == "ensure":
            ensure_collection(Path(args.db), args.collection, args.dim)
            print_json({"ok": True, "db": args.db, "collection": args.collection})
        elif args.command == "upsert":
            payload = json.load(sys.stdin)
            ensure_collection(Path(args.db), args.collection, args.dim)
            upsert_record(Path(args.db), args.collection, args.dim, payload)
            print_json({"ok": True, "uuid": payload["record"]["uuid"]})
        elif args.command == "list":
            print_json(
                {
                    "ok": True,
                    "records": list_records_from_collection_uri(
                        source_uri(args),
                        args.collection,
                    ),
                }
            )
        elif args.command == "get":
            print_json(
                {
                    "ok": True,
                    "record": get_record_from_uri(source_uri(args), args.collection, args.id),
                }
            )
        elif args.command == "delete":
            delete_record(Path(args.db), args.collection, args.id)
            print_json({"ok": True, "deleted": True, "uuid": args.id})
        elif args.command == "pending":
            records = [
                record
                for record in list_records_from_collection(Path(args.db), args.collection)
                if record.get("embedding_status") == "pending"
                or (args.retry_failed and record.get("embedding_status") == "failed")
            ]
            print_json({"ok": True, "records": records[: args.limit]})
        elif args.command == "search":
            payload = json.load(sys.stdin)
            print_json(
                {
                    "ok": True,
                    "hits": search_vectors(
                        source_uri(args),
                        args.collection,
                        payload["vector"],
                        args.limit,
                    ),
                }
            )
    except Exception as exc:
        print_json({"ok": False, "error": str(exc)}, stream=sys.stderr)
        return 1
    return 0


def source_uri(args: argparse.Namespace) -> str:
    raw = args.uri or args.db
    if not raw:
        raise ValueError("either --db or --uri is required")
    return str(raw)


def ensure_collection(db_path: Path, collection: str, dim: int) -> None:
    from pymilvus import DataType, MilvusClient

    db_path.parent.mkdir(parents=True, exist_ok=True)
    client = MilvusClient(uri=str(db_path))
    if client.has_collection(collection_name=collection):
        return

    schema = client.create_schema(auto_id=False, enable_dynamic_field=True)
    schema.add_field("uuid", DataType.VARCHAR, is_primary=True, max_length=64)
    schema.add_field("content", DataType.VARCHAR, max_length=8192)
    schema.add_field("keys", DataType.VARCHAR, max_length=4096)
    schema.add_field("summary", DataType.VARCHAR, max_length=1024)
    schema.add_field("embedding_status", DataType.VARCHAR, max_length=32)
    schema.add_field("embedding_error", DataType.VARCHAR, max_length=1024)
    schema.add_field("embedding_attempts", DataType.INT64)
    schema.add_field("memory_type", DataType.VARCHAR, max_length=32)
    schema.add_field("scope", DataType.VARCHAR, max_length=32)
    schema.add_field("root_path", DataType.VARCHAR, max_length=2048)
    schema.add_field("tags", DataType.VARCHAR, max_length=2048)
    schema.add_field("source_kind", DataType.VARCHAR, max_length=32)
    schema.add_field("source_ref", DataType.VARCHAR, max_length=2048)
    schema.add_field("created_at", DataType.VARCHAR, max_length=64)
    schema.add_field("updated_at", DataType.VARCHAR, max_length=64)
    schema.add_field("last_accessed_at", DataType.VARCHAR, max_length=64)
    schema.add_field("access_count", DataType.INT64)
    schema.add_field("conflict_count", DataType.INT64)
    schema.add_field("confidence", DataType.DOUBLE)
    schema.add_field("verified_at", DataType.VARCHAR, max_length=64)
    schema.add_field("stale_after_days", DataType.INT64)
    schema.add_field("embedding_provider", DataType.VARCHAR, max_length=64)
    schema.add_field("embedding_model", DataType.VARCHAR, max_length=256)
    schema.add_field("embedding_dim", DataType.INT64)
    schema.add_field("schema_version", DataType.INT64)
    schema.add_field("reliability", DataType.DOUBLE)
    schema.add_field("dense_vector", DataType.FLOAT_VECTOR, dim=dim)

    index_params = client.prepare_index_params()
    index_params.add_index(
        field_name="dense_vector",
        index_type="AUTOINDEX",
        metric_type="COSINE",
    )
    client.create_collection(
        collection_name=collection,
        schema=schema,
        index_params=index_params,
    )


def upsert_record(
    db_path: Path,
    collection: str,
    dim: int,
    payload: dict[str, Any],
) -> None:
    from pymilvus import MilvusClient

    record = payload["record"]
    if not isinstance(record, dict):
        raise ValueError("payload must contain record object")

    raw_vector = payload.get("vector")
    if isinstance(raw_vector, list):
        vector = [float(value) for value in raw_vector]
    else:
        vector = existing_vector(db_path, collection, str(record["uuid"])) or [0.0] * dim

    client = MilvusClient(uri=str(db_path))
    client.upsert(
        collection_name=collection,
        data=[entity_for_record(record, vector, payload.get("reliability"))],
    )
    client.flush(collection_name=collection)


def list_records_from_collection(db_path: Path, collection: str) -> list[dict[str, Any]]:
    return list_records_from_collection_uri(str(db_path), collection)


def list_records_from_collection_uri(uri: str, collection: str) -> list[dict[str, Any]]:
    from pymilvus import MilvusClient

    client = MilvusClient(uri=uri)
    if not client.has_collection(collection_name=collection):
        return []
    client.load_collection(collection_name=collection)
    rows = client.query(
        collection_name=collection,
        filter="",
        output_fields=RECORD_OUTPUT_FIELDS,
    )
    return [record_from_entity(row) for row in rows]


def get_record(db_path: Path, collection: str, uuid: str) -> dict[str, Any] | None:
    return get_record_from_uri(str(db_path), collection, uuid)


def get_record_from_uri(uri: str, collection: str, uuid: str) -> dict[str, Any] | None:
    from pymilvus import MilvusClient

    client = MilvusClient(uri=uri)
    if not client.has_collection(collection_name=collection):
        return None
    client.load_collection(collection_name=collection)
    rows = client.get(
        collection_name=collection,
        ids=[uuid],
        output_fields=RECORD_OUTPUT_FIELDS,
    )
    if not rows:
        return None
    return record_from_entity(rows[0])


def delete_record(db_path: Path, collection: str, uuid: str) -> None:
    from pymilvus import MilvusClient

    client = MilvusClient(uri=str(db_path))
    if client.has_collection(collection_name=collection):
        client.load_collection(collection_name=collection)
        client.delete(collection_name=collection, ids=[uuid])
        client.flush(collection_name=collection)


def existing_vector(db_path: Path, collection: str, uuid: str) -> list[float] | None:
    from pymilvus import MilvusClient

    client = MilvusClient(uri=str(db_path))
    if not client.has_collection(collection_name=collection):
        return None
    client.load_collection(collection_name=collection)
    rows = client.get(
        collection_name=collection,
        ids=[uuid],
        output_fields=[VECTOR_FIELD],
    )
    if not rows:
        return None
    vector = rows[0].get(VECTOR_FIELD)
    if not isinstance(vector, list):
        return None
    return [float(value) for value in vector]


def search_vectors(
    uri: str | Path,
    collection: str,
    vector: list[float],
    limit: int,
) -> list[dict[str, Any]]:
    from pymilvus import MilvusClient

    client = MilvusClient(uri=str(uri))
    if not client.has_collection(collection_name=collection):
        return []
    client.load_collection(collection_name=collection)
    results = client.search(
        collection_name=collection,
        data=[[float(value) for value in vector]],
        anns_field="dense_vector",
        filter='embedding_status == "embedded"',
        limit=limit,
        output_fields=[PRIMARY_FIELD],
        search_params={"metric_type": "COSINE", "params": {}},
    )
    hits: list[dict[str, Any]] = []
    for hit in results[0] if results else []:
        entity = dict(hit.get("entity") or {})
        entity["distance"] = float(hit.get("distance", 0.0))
        hits.append(entity)
    return hits


def entity_for_record(
    record: dict[str, Any],
    vector: list[float],
    reliability: Any,
) -> dict[str, Any]:
    return {
        "uuid": str(record.get("uuid") or record.get("memory_id") or ""),
        "content": str(record.get("content") or ""),
        "keys": json.dumps(record.get("keys") or [], ensure_ascii=False),
        "summary": str(record.get("summary") or ""),
        "embedding_status": str(record.get("embedding_status") or "pending"),
        "embedding_error": str(record.get("embedding_error") or ""),
        "embedding_attempts": int(record.get("embedding_attempts") or 0),
        "memory_type": str(record.get("memory_type") or "project"),
        "scope": str(record.get("scope") or "project"),
        "root_path": str(record.get("root_path") or ""),
        "tags": json.dumps(record.get("tags") or [], ensure_ascii=False),
        "source_kind": str(record.get("source_kind") or "agent_inferred"),
        "source_ref": str(record.get("source_ref") or ""),
        "created_at": str(record.get("created_at") or ""),
        "updated_at": str(record.get("updated_at") or ""),
        "last_accessed_at": str(record.get("last_accessed_at") or ""),
        "access_count": int(record.get("access_count") or 0),
        "conflict_count": int(record.get("conflict_count") or 0),
        "confidence": float(record.get("confidence") or 0.0),
        "verified_at": str(record.get("verified_at") or ""),
        "stale_after_days": int(record.get("stale_after_days") or -1),
        "embedding_provider": str(record.get("embedding_provider") or ""),
        "embedding_model": str(record.get("embedding_model") or ""),
        "embedding_dim": int(record.get("embedding_dim") or len(vector)),
        "schema_version": int(record.get("schema_version") or 1),
        "reliability": float(reliability or 0.0),
        "dense_vector": vector,
    }


def print_json(data: dict[str, Any], *, stream=sys.stdout) -> None:
    stream.write(json.dumps(data, ensure_ascii=False, sort_keys=True) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
