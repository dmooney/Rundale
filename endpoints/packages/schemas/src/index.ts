import AjvModule, { type ErrorObject, type ValidateFunction } from "ajv";
import addFormatsModule from "ajv-formats";
import type { JsonSchema } from "@parish/domain";

const supportedKeywords = new Set([
  "$schema",
  "$id",
  "type",
  "properties",
  "required",
  "items",
  "anyOf",
  "additionalProperties",
  "enum",
  "minLength",
  "maxLength",
  "maxItems",
  "minimum",
  "maximum",
  "description",
  "title",
  "default",
  "contentMediaType",
  "x-semantic-type",
]);

const supportedTypes = new Set([
  "object",
  "array",
  "string",
  "integer",
  "number",
  "boolean",
  "null",
]);
const semanticTypes = new Set(["image", "text", "json"]);

function isSchemaObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasObjectType(type: unknown): boolean {
  return type === "object" || (Array.isArray(type) && type.includes("object"));
}

export interface SchemaIssue {
  path: string;
  message: string;
}

export class SchemaDefinitionError extends Error {
  constructor(public readonly issues: SchemaIssue[]) {
    super("JSON Schema uses invalid or unsupported constructs.");
    this.name = "SchemaDefinitionError";
  }
}

interface SchemaInspectionContext {
  topLevelImageFields: number;
}

function inspectSchema(
  schema: unknown,
  path = "$",
  context: SchemaInspectionContext = { topLevelImageFields: 0 },
  allowTopLevelImage = false,
): SchemaIssue[] {
  if (schema === true || schema === false) {
    return [{ path, message: "Boolean schemas are not supported." }];
  }
  if (!isSchemaObject(schema)) {
    return [{ path, message: "Schema nodes must be objects." }];
  }

  const record = schema;
  const issues: SchemaIssue[] = [];
  for (const key of Object.keys(record)) {
    if (!supportedKeywords.has(key)) {
      issues.push({ path: `${path}.${key}`, message: `Keyword '${key}' is not supported.` });
    }
  }

  if (record.anyOf !== undefined) {
    if (!Array.isArray(record.anyOf)) {
      issues.push({ path: `${path}.anyOf`, message: "anyOf must be an array." });
    } else if (record.anyOf.length !== 2) {
      issues.push({
        path: `${path}.anyOf`,
        message: "anyOf must contain exactly one typed branch and one null branch.",
      });
    } else {
      const nullBranchCount = record.anyOf.filter(
        (branch) => isSchemaObject(branch) && branch.type === "null",
      ).length;
      if (nullBranchCount !== 1) {
        issues.push({
          path: `${path}.anyOf`,
          message: "anyOf must contain exactly one null branch.",
        });
      }

      const typedBranch = record.anyOf.find(
        (branch) => !(isSchemaObject(branch) && branch.type === "null"),
      );
      if (
        !isSchemaObject(typedBranch) ||
        typeof typedBranch.type !== "string" ||
        typedBranch.type === "null" ||
        !supportedTypes.has(typedBranch.type)
      ) {
        issues.push({
          path: `${path}.anyOf`,
          message: "anyOf must contain one supported typed branch and one null branch.",
        });
      }
    }

    if (Object.hasOwn(record, "type")) {
      issues.push({
        path: `${path}.type`,
        message: "Nullable anyOf nodes must not declare type.",
      });
    }
    if (Array.isArray(record.anyOf)) {
      for (const [index, branch] of record.anyOf.entries()) {
        issues.push(...inspectSchema(branch, `${path}.anyOf.${index}`, context));
      }
    }
  } else if (!Object.hasOwn(record, "type")) {
    issues.push({ path: `${path}.type`, message: "Schema nodes must declare type." });
  } else {
    const types = Array.isArray(record.type) ? record.type : [record.type];
    if (types.length === 0) {
      issues.push({ path: `${path}.type`, message: "Schema type must not be empty." });
    }
    for (const type of types) {
      if (typeof type !== "string" || !supportedTypes.has(type)) {
        issues.push({ path: `${path}.type`, message: `Type '${String(type)}' is not supported.` });
      }
    }
  }

  if (record.maxItems !== undefined) {
    if (
      typeof record.maxItems !== "number" ||
      !Number.isSafeInteger(record.maxItems) ||
      record.maxItems < 0
    ) {
      issues.push({
        path: `${path}.maxItems`,
        message: "maxItems must be a non-negative safe integer.",
      });
    }
  }

  if (record["x-semantic-type"] !== undefined) {
    if (!semanticTypes.has(String(record["x-semantic-type"]))) {
      issues.push({ path, message: "Unsupported x-semantic-type." });
    }
    if (record["x-semantic-type"] === "image" && record.type !== "string") {
      issues.push({ path, message: "Image fields must use JSON Schema type 'string'." });
    }
    if (record["x-semantic-type"] === "image") {
      if (!allowTopLevelImage) {
        issues.push({
          path,
          message: "Image fields must be direct properties of the top-level object.",
        });
      } else {
        context.topLevelImageFields += 1;
        if (context.topLevelImageFields > 1) {
          issues.push({ path, message: "Only one top-level image field is supported." });
        }
      }
    }
  }

  if (record.properties !== undefined) {
    if (
      record.properties === null ||
      typeof record.properties !== "object" ||
      Array.isArray(record.properties)
    ) {
      issues.push({ path: `${path}.properties`, message: "properties must be an object." });
    } else {
      for (const [key, child] of Object.entries(record.properties)) {
        issues.push(
          ...inspectSchema(
            child,
            `${path}.properties.${key}`,
            context,
            path === "$" && hasObjectType(record.type),
          ),
        );
      }
    }
  }
  if (record.items !== undefined) {
    issues.push(...inspectSchema(record.items, `${path}.items`, context));
  }
  if (record.additionalProperties !== undefined && isSchemaObject(record.additionalProperties)) {
    issues.push(
      ...inspectSchema(record.additionalProperties, `${path}.additionalProperties`, context),
    );
  }
  return issues;
}

const ajv = new AjvModule.default({ allErrors: true, strict: false });
addFormatsModule.default(ajv);

export function validateSchemaDefinition(schema: JsonSchema): void {
  compileCheckedSchema(schema);
}

function compileCheckedSchema(schema: JsonSchema): ValidateFunction {
  const issues = inspectSchema(schema);
  if (issues.length > 0) throw new SchemaDefinitionError(issues);
  try {
    const validate = ajv.compile(schema);
    // Ajv keys its cache by the schema object. Definitions are self-contained
    // in this subset, so retaining each caller-supplied object would grow the
    // process cache for every distinct draft or invocation schema.
    ajv.removeSchema(schema);
    return validate;
  } catch (error) {
    ajv.removeSchema(schema);
    throw new SchemaDefinitionError([{ path: "$", message: String(error) }]);
  }
}

export function compileSchema(schema: JsonSchema): ValidateFunction {
  return compileCheckedSchema(schema);
}

export function formatValidationErrors(errors: ErrorObject[] | null | undefined): SchemaIssue[] {
  return (errors ?? []).map((error) => ({
    path: error.instancePath === "" ? "$" : `$${error.instancePath.replaceAll("/", ".")}`,
    message: error.message ?? "does not match the schema",
  }));
}
