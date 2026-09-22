import {
  type IBinaryKeyData,
  type IDataObject,
  type IExecuteFunctions,
  type INodeExecutionData,
  type INodeType,
  type INodeTypeDescription,
  NodeConnectionTypes,
  NodeOperationError,
} from "n8n-workflow";

import {
  extract,
  extractBatch,
  mapUrl,
  type ExtractInput,
  type ExtractionConfig,
  type UrlExtractionConfig,
} from "@xberg-io/xberg";

import { DEFAULT_CHUNK_OVERLAP, DEFAULT_CHUNK_SIZE } from "./Xberg.node.constants";
import { xbergNodeProperties } from "./Xberg.node.properties";

// The binding's config types are deeply `readonly` (1.0.7 API tightening), but these
// nodes build the config incrementally by assignment. Strip the top-level `readonly`
// for the local builder; a `Writable<T>` is still assignable to the `readonly` `T` the
// `extract` / `mapUrl` calls expect. ~keep
type Writable<T> = { -readonly [P in keyof T]: T[P] };

// Output format identifiers mirror the `OutputFormat` string enum exported by
// `@xberg-io/xberg`. Kept as a local alias so the node compiles against the
// binding's type without importing the runtime enum value. ~keep
type XbergOutputFormat = NonNullable<ExtractionConfig["outputFormat"]>;

// MIME type emitted for the optional binary output, keyed by output format. ~keep
const OUTPUT_MIME_TYPES: Record<string, string> = {
  plain: "text/plain",
  markdown: "text/markdown",
  djot: "text/plain",
  html: "text/html",
  json: "application/json",
  doctags: "text/plain",
};

const OUTPUT_EXTENSIONS: Record<string, string> = {
  plain: "txt",
  markdown: "md",
  djot: "dj",
  html: "html",
  json: "json",
  doctags: "dt",
};

// A single extracted document as returned by the binding. Field types are kept
// loose because the binding's generated `.d.ts` names them via unexported
// aliases; the node only reads a documented subset. ~keep
interface XbergDocument {
  content?: string;
  mimeType?: string;
  extractionMethod?: string;
  detectedLanguages?: string[];
  qualityScore?: number;
  counts?: unknown;
  metadata?: unknown;
  tables?: unknown[];
  chunks?: unknown[];
  images?: unknown[];
  entities?: unknown[];
  summary?: unknown;
}

interface XbergError {
  index: number;
  message: string;
}

function parseOcrLanguages(raw: string): string[] {
  const languages = raw
    .split(",")
    .map((code) => code.trim())
    .filter((code) => code.length > 0);
  return languages.length > 0 ? languages : ["eng"];
}

// Translate the node's option collection into an `ExtractionConfig` accepted by
// the binding. Only options the user opted into are set so the binding's own
// defaults apply everywhere else. ~keep
function buildExtractionConfig(outputFormat: XbergOutputFormat, options: IDataObject): ExtractionConfig {
  const enableOcr = options.enableOcr !== false;

  const config: Writable<ExtractionConfig> = {
    outputFormat,
    forceOcr: enableOcr ? options.forceOcr === true : false,
    disableOcr: !enableOcr,
    enableQualityProcessing: options.enableQualityProcessing === true,
  };

  if (enableOcr) {
    config.ocr = { enabled: true, language: parseOcrLanguages((options.ocrLanguages as string) || "eng") };
  }
  if (options.enableChunking === true) {
    config.chunking = {
      maxCharacters: (options.chunkSize as number) || DEFAULT_CHUNK_SIZE,
      overlap: (options.chunkOverlap as number) ?? DEFAULT_CHUNK_OVERLAP,
    };
  }
  if (options.extractImages === true) {
    config.images = { extractImages: true };
  }

  return config;
}

// Build the per-item output JSON. `content` plus cheap structural signals are
// always emitted; heavier collections are gated on the matching include option. ~keep
function documentToJson(document: XbergDocument, options: IDataObject): IDataObject {
  const outputField = (options.outputField as string) || "text";

  const json: IDataObject = {
    [outputField]: document.content ?? "",
    mimeType: document.mimeType,
    extractionMethod: document.extractionMethod,
    detectedLanguages: document.detectedLanguages,
    counts: document.counts as IDataObject,
  };

  if (typeof document.qualityScore === "number") {
    json.qualityScore = document.qualityScore;
  }
  if (options.includeMetadata !== false && document.metadata) {
    json.metadata = document.metadata as IDataObject;
  }
  if (options.includeTables === true && document.tables) {
    json.tables = document.tables as IDataObject[];
  }
  if (options.includeChunks === true && document.chunks) {
    json.chunks = document.chunks as IDataObject[];
  }
  if (document.entities) {
    json.entities = document.entities as IDataObject[];
  }
  if (document.summary) {
    json.summary = document.summary as IDataObject;
  }

  return json;
}

async function buildBinaryOutput(
  context: IExecuteFunctions,
  document: XbergDocument,
  fileName: string | undefined,
  outputFormat: XbergOutputFormat,
  options: IDataObject,
): Promise<IBinaryKeyData> {
  const binaryOutputField = (options.binaryOutputField as string) || "data";
  const mimeType = OUTPUT_MIME_TYPES[outputFormat] ?? "text/plain";
  const extension = OUTPUT_EXTENSIONS[outputFormat] ?? "txt";
  const baseName = (fileName ?? "document").replace(/\.[^.]+$/, "");

  return {
    [binaryOutputField]: await context.helpers.prepareBinaryData(
      Buffer.from(document.content ?? "", "utf-8"),
      `${baseName}.${extension}`,
      mimeType,
    ),
  };
}

// Resolve the extraction input for one item from either uploaded binary data or
// a URL/path string, per the node's Input Source parameter. ~keep
async function buildExtractInput(
  context: IExecuteFunctions,
  itemIndex: number,
): Promise<{ input: ExtractInput; fileName?: string }> {
  const inputSource = context.getNodeParameter("inputSource", itemIndex, "binary") as string;

  if (inputSource === "url") {
    const uri = (context.getNodeParameter("sourceUri", itemIndex, "") as string).trim();
    if (!uri) {
      throw new NodeOperationError(context.getNode(), "No URL or path provided for extraction", { itemIndex });
    }
    return { input: { kind: "uri" as NonNullable<ExtractInput["kind"]>, uri } };
  }

  const binaryPropertyName = context.getNodeParameter("binaryPropertyName", itemIndex) as string;
  const binaryData = context.helpers.assertBinaryData(itemIndex, binaryPropertyName);
  const buffer = await context.helpers.getBinaryDataBuffer(itemIndex, binaryPropertyName);

  return {
    input: {
      // The binding requires an explicit source kind; "bytes" reads from the
      // in-memory `bytes` field rather than fetching a URI. ~keep
      kind: "bytes" as NonNullable<ExtractInput["kind"]>,
      bytes: new Uint8Array(buffer),
      filename: binaryData.fileName,
      mimeType: binaryData.mimeType,
    },
    fileName: binaryData.fileName,
  };
}

function errorItem(context: IExecuteFunctions, itemIndex: number, error: unknown): INodeExecutionData {
  return {
    json: context.getInputData(itemIndex)[0].json,
    error: error as NodeOperationError,
    pairedItem: { item: itemIndex },
  };
}

async function runExtract(context: IExecuteFunctions): Promise<INodeExecutionData[]> {
  const items = context.getInputData();
  const returnData: INodeExecutionData[] = [];

  for (let itemIndex = 0; itemIndex < items.length; itemIndex++) {
    try {
      const outputFormat = context.getNodeParameter("outputFormat", itemIndex) as XbergOutputFormat;
      const options = context.getNodeParameter("options", itemIndex, {}) as IDataObject;
      const config = buildExtractionConfig(outputFormat, options);

      const { input, fileName } = await buildExtractInput(context, itemIndex);
      const result = await extract(input, config);

      const firstError = result.errors?.[0];
      if (firstError) {
        throw new NodeOperationError(context.getNode(), `Xberg extraction failed: ${firstError.message}`, {
          itemIndex,
        });
      }
      const document = result.results?.[0] as XbergDocument | undefined;
      if (!document) {
        throw new NodeOperationError(context.getNode(), "Xberg returned no extracted document for the input", {
          itemIndex,
        });
      }

      const outputItem: INodeExecutionData = {
        json: documentToJson(document, options),
        pairedItem: { item: itemIndex },
      };
      if (options.returnBinary === true) {
        outputItem.binary = await buildBinaryOutput(context, document, fileName, outputFormat, options);
      }
      returnData.push(outputItem);
    } catch (error) {
      if (context.continueOnFail()) {
        returnData.push(errorItem(context, itemIndex, error));
        continue;
      }
      throw error;
    }
  }

  return returnData;
}

// Extract every input item in a single `extractBatch` call, which the binding
// schedules concurrently and is substantially faster than looping `extract`. ~keep
async function runExtractBatch(context: IExecuteFunctions): Promise<INodeExecutionData[]> {
  const items = context.getInputData();
  const outputFormat = context.getNodeParameter("outputFormat", 0) as XbergOutputFormat;
  const options = context.getNodeParameter("options", 0, {}) as IDataObject;
  const config = buildExtractionConfig(outputFormat, options);

  const inputs: ExtractInput[] = [];
  const fileNames: Array<string | undefined> = [];
  for (let itemIndex = 0; itemIndex < items.length; itemIndex++) {
    const { input, fileName } = await buildExtractInput(context, itemIndex);
    inputs.push(input);
    fileNames.push(fileName);
  }

  const result = await extractBatch(inputs, config);
  const errorsByIndex = new Map<number, XbergError>();
  for (const error of (result.errors ?? []) as XbergError[]) {
    errorsByIndex.set(error.index, error);
  }

  // Results come back in ascending input order with errored inputs omitted, so
  // a single forward cursor keeps successful documents aligned to their item. ~keep
  const documents = (result.results ?? []) as XbergDocument[];
  const returnData: INodeExecutionData[] = [];
  let cursor = 0;

  for (let itemIndex = 0; itemIndex < items.length; itemIndex++) {
    const failure = errorsByIndex.get(itemIndex);
    if (failure) {
      const error = new NodeOperationError(context.getNode(), `Xberg extraction failed: ${failure.message}`, {
        itemIndex,
      });
      if (context.continueOnFail()) {
        returnData.push(errorItem(context, itemIndex, error));
        continue;
      }
      throw error;
    }

    const document = documents[cursor++];
    if (!document) {
      const error = new NodeOperationError(context.getNode(), "Xberg returned no extracted document for the input", {
        itemIndex,
      });
      if (context.continueOnFail()) {
        returnData.push(errorItem(context, itemIndex, error));
        continue;
      }
      throw error;
    }

    const outputItem: INodeExecutionData = {
      json: documentToJson(document, options),
      pairedItem: { item: itemIndex },
    };
    if (options.returnBinary === true) {
      outputItem.binary = await buildBinaryOutput(context, document, fileNames[itemIndex], outputFormat, options);
    }
    returnData.push(outputItem);
  }

  return returnData;
}

async function runMapUrl(context: IExecuteFunctions): Promise<INodeExecutionData[]> {
  const items = context.getInputData();
  const returnData: INodeExecutionData[] = [];

  for (let itemIndex = 0; itemIndex < items.length; itemIndex++) {
    try {
      const uri = (context.getNodeParameter("mapUri", itemIndex, "") as string).trim();
      if (!uri) {
        throw new NodeOperationError(context.getNode(), "No URL provided to map", { itemIndex });
      }
      const mapOptions = context.getNodeParameter("mapOptions", itemIndex, {}) as IDataObject;

      const config: Writable<UrlExtractionConfig> = {};
      if (mapOptions.mode) {
        config.mode = mapOptions.mode as NonNullable<UrlExtractionConfig["mode"]>;
      }
      if (mapOptions.maxTotalUrls) {
        config.maxTotalUrls = mapOptions.maxTotalUrls as number;
      }

      const result = await mapUrl(uri, config);
      returnData.push({
        json: { url: uri, urls: (result.urls ?? []) as IDataObject[] },
        pairedItem: { item: itemIndex },
      });
    } catch (error) {
      if (context.continueOnFail()) {
        returnData.push(errorItem(context, itemIndex, error));
        continue;
      }
      throw error;
    }
  }

  return returnData;
}

export class Xberg implements INodeType {
  description: INodeTypeDescription = {
    displayName: "Xberg",
    name: "xberg",
    icon: "file:xberg.svg",
    group: ["transform"],
    version: 1,
    subtitle: '={{ $parameter["operation"] }}',
    description: "Extract text, tables, and metadata from documents with Xberg",
    defaults: {
      name: "Xberg",
    },
    inputs: [NodeConnectionTypes.Main],
    outputs: [NodeConnectionTypes.Main],
    credentials: [],
    properties: xbergNodeProperties,
  };

  async execute(this: IExecuteFunctions): Promise<INodeExecutionData[][]> {
    const operation = this.getNodeParameter("operation", 0) as string;

    if (operation === "mapUrl") {
      return [await runMapUrl(this)];
    }
    if (operation === "extractBatch") {
      return [await runExtractBatch(this)];
    }
    return [await runExtract(this)];
  }
}
