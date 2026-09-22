import { type INodeProperties } from "n8n-workflow";

import { DEFAULT_CHUNK_OVERLAP, DEFAULT_CHUNK_SIZE } from "./Xberg.node.constants";

// The node's parameter/options schema, split out from Xberg.node.ts to keep that file under
// the repo's type-length threshold. Order is load-bearing for the rendered n8n UI: entries
// must stay in the exact order they were declared in the original file. ~keep
export const xbergNodeProperties: INodeProperties[] = [
  {
    displayName: "Resource",
    name: "resource",
    type: "options",
    noDataExpression: true,
    options: [
      {
        name: "Document",
        value: "document",
      },
    ],
    default: "document",
  },
  {
    displayName: "Operation",
    name: "operation",
    type: "options",
    noDataExpression: true,
    displayOptions: {
      show: {
        resource: ["document"],
      },
    },
    options: [
      {
        name: "Extract",
        value: "extract",
        action: "Extract text and metadata from a document",
        description: "Extract text, tables, and metadata from one document per item",
      },
      {
        name: "Extract Batch",
        value: "extractBatch",
        action: "Extract every input item in one fast batch",
        description: "Extract all incoming items in a single batch call, faster than looping Extract",
      },
      {
        name: "Map URL",
        value: "mapUrl",
        action: "Discover links from a page or sitemap",
        description: "List the URLs reachable from a web page or sitemap without extracting them",
      },
    ],
    default: "extract",
  },
  {
    displayName: "Input Source",
    name: "inputSource",
    type: "options",
    displayOptions: {
      show: {
        operation: ["extract", "extractBatch"],
      },
    },
    options: [
      {
        name: "Binary Data",
        value: "binary",
        description: "Read the document from a binary property on the incoming item",
      },
      {
        name: "URL or Path",
        value: "url",
        description: "Fetch the document from an HTTP(S) URL or read it from a local path",
      },
    ],
    default: "binary",
    description: "Where the document to extract comes from",
  },
  {
    displayName: "Input Binary Field",
    name: "binaryPropertyName",
    type: "string",
    default: "data",
    required: true,
    displayOptions: {
      show: {
        operation: ["extract", "extractBatch"],
        inputSource: ["binary"],
      },
    },
    description: "Name of the binary property on the incoming item that holds the document to extract",
  },
  {
    displayName: "URL or Path",
    name: "sourceUri",
    type: "string",
    default: "",
    required: true,
    displayOptions: {
      show: {
        operation: ["extract", "extractBatch"],
        inputSource: ["url"],
      },
    },
    placeholder: "https://example.com/report.pdf",
    description: "HTTP(S) URL or local filesystem path of the document to extract",
  },
  {
    displayName: "URL",
    name: "mapUri",
    type: "string",
    default: "",
    required: true,
    displayOptions: {
      show: {
        operation: ["mapUrl"],
      },
    },
    placeholder: "https://example.com",
    description: "Web page or sitemap URL to discover links from",
  },
  {
    displayName: "Output Format",
    name: "outputFormat",
    type: "options",
    displayOptions: {
      show: {
        operation: ["extract", "extractBatch"],
      },
    },
    options: [
      {
        name: "Djot",
        value: "djot",
      },
      {
        name: "DocTags",
        value: "doctags",
      },
      {
        name: "HTML",
        value: "html",
      },
      {
        name: "JSON Tree",
        value: "json",
      },
      {
        name: "Markdown",
        value: "markdown",
      },
      {
        name: "Plain Text",
        value: "plain",
      },
    ],
    default: "markdown",
    description: "Format of the extracted text content",
  },
  {
    displayName: "Options",
    name: "options",
    type: "collection",
    placeholder: "Add Option",
    default: {},
    displayOptions: {
      show: {
        operation: ["extract", "extractBatch"],
      },
    },
    options: [
      {
        displayName: "Binary Output Field",
        name: "binaryOutputField",
        type: "string",
        default: "data",
        displayOptions: {
          show: {
            returnBinary: [true],
          },
        },
        description: "Name of the binary property to attach the extracted content to",
      },
      {
        displayName: "Chunk Overlap",
        name: "chunkOverlap",
        type: "number",
        default: DEFAULT_CHUNK_OVERLAP,
        displayOptions: {
          show: {
            enableChunking: [true],
          },
        },
        description: "Number of overlapping characters between adjacent chunks",
      },
      {
        displayName: "Chunk Size",
        name: "chunkSize",
        type: "number",
        default: DEFAULT_CHUNK_SIZE,
        displayOptions: {
          show: {
            enableChunking: [true],
          },
        },
        description: "Maximum number of characters per chunk",
      },
      {
        displayName: "Enable Chunking",
        name: "enableChunking",
        type: "boolean",
        default: false,
        description: "Whether to split the extracted content into overlapping chunks for RAG pipelines",
      },
      {
        displayName: "Enable OCR",
        name: "enableOcr",
        type: "boolean",
        default: true,
        description:
          "Whether to run OCR on images and scanned PDF pages. Disable for text-only documents to skip OCR entirely.",
      },
      {
        displayName: "Enable Quality Processing",
        name: "enableQualityProcessing",
        type: "boolean",
        default: false,
        description: "Whether to run quality post-processing to clean up the extracted text",
      },
      {
        displayName: "Extract Images",
        name: "extractImages",
        type: "boolean",
        default: false,
        description: "Whether to extract embedded images and report them in the output",
      },
      {
        displayName: "Force OCR",
        name: "forceOcr",
        type: "boolean",
        default: false,
        description:
          "Whether to force OCR on every page even when a usable text layer is present. Ignored when OCR is disabled.",
      },
      {
        displayName: "Include Chunks",
        name: "includeChunks",
        type: "boolean",
        default: false,
        description: "Whether to include the generated chunks in the output. Requires Enable Chunking.",
      },
      {
        displayName: "Include Metadata",
        name: "includeMetadata",
        type: "boolean",
        default: true,
        description: "Whether to include document metadata (title, author, dates) in the output",
      },
      {
        displayName: "Include Tables",
        name: "includeTables",
        type: "boolean",
        default: false,
        description: "Whether to include structured table data in the output",
      },
      {
        displayName: "OCR Languages",
        name: "ocrLanguages",
        type: "string",
        default: "eng",
        description: 'Comma-separated ISO 639-2 language codes for OCR recognition, for example "eng,deu"',
      },
      {
        displayName: "Output Content Field",
        name: "outputField",
        type: "string",
        default: "text",
        description: "Name of the JSON field to write the extracted content into",
      },
      {
        displayName: "Return As Binary",
        name: "returnBinary",
        type: "boolean",
        default: false,
        description: "Whether to also attach the extracted content as a binary property on the output item",
      },
    ],
  },
  {
    displayName: "Options",
    name: "mapOptions",
    type: "collection",
    placeholder: "Add Option",
    default: {},
    displayOptions: {
      show: {
        operation: ["mapUrl"],
      },
    },
    options: [
      {
        displayName: "Max Total URLs",
        name: "maxTotalUrls",
        type: "number",
        default: 0,
        description: "Maximum number of URLs to return. 0 uses the binding default.",
      },
      {
        displayName: "Mode",
        name: "mode",
        type: "options",
        options: [
          {
            name: "Auto",
            value: "auto",
            description: "Classify the resource after fetching it",
          },
          {
            name: "Crawl",
            value: "crawl",
            description: "Crawl from the seed URL and collect discovered links",
          },
          {
            name: "Document",
            value: "document",
            description: "Treat the URL as a single document or page",
          },
        ],
        default: "auto",
        description: "How the URL is interpreted while discovering links",
      },
    ],
  },
];
