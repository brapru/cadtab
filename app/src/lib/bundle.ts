// The project-bundle format: a whole multi-file cadtab project serialized as one
// file so it can move (download/upload) as a unit and populate the in-memory
// file provider in the browser. JSON for the MVP; the `version` tag lets a
// zip-based container replace it later without breaking older readers.

export interface ProjectBundle {
  // The path (a key in `files`) of the score the editor opens.
  entry: string;
  // Every file in the project, path -> contents, including the entry.
  files: Record<string, string>;
}

const VERSION = 1;

interface BundleEnvelope {
  version: number;
  entry: string;
  files: Record<string, string>;
}

/// Serialize a bundle to its on-disk JSON form.
export function serializeBundle(bundle: ProjectBundle): string {
  const envelope: BundleEnvelope = {
    version: VERSION,
    entry: bundle.entry,
    files: bundle.files,
  };
  return JSON.stringify(envelope, null, 2);
}

/// Parse a bundle from JSON, validating its shape. Throws a descriptive Error on
/// anything malformed so the caller can surface it to the user.
export function parseBundle(text: string): ProjectBundle {
  let data: unknown;
  try {
    data = JSON.parse(text);
  } catch {
    throw new Error("not a valid project bundle (invalid JSON)");
  }
  if (typeof data !== "object" || data === null) {
    throw new Error("not a valid project bundle");
  }

  const obj = data as Record<string, unknown>;
  if (typeof obj.version === "number" && obj.version > VERSION) {
    throw new Error(
      `project bundle version ${obj.version} is newer than this app supports`,
    );
  }
  if (typeof obj.entry !== "string") {
    throw new Error("project bundle is missing a string `entry`");
  }
  if (typeof obj.files !== "object" || obj.files === null) {
    throw new Error("project bundle is missing a `files` map");
  }

  const files: Record<string, string> = {};
  for (const [path, contents] of Object.entries(
    obj.files as Record<string, unknown>,
  )) {
    if (typeof contents !== "string") {
      throw new Error(`project bundle file "${path}" is not text`);
    }
    files[path] = contents;
  }
  if (!(obj.entry in files)) {
    throw new Error(
      `project bundle entry "${obj.entry}" is not among its files`,
    );
  }

  return { entry: obj.entry, files };
}
