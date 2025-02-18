import { RefreshRuntimeGlobals } from "@next/react-refresh-utils/dist/runtime";
import { ServerMessage } from "./protocol";
import { Hot } from "./hot";
import { DevRuntimeParams } from "./runtime";

export type RefreshHelpers = RefreshRuntimeGlobals["$RefreshHelpers$"];

type ChunkPath = string;
type ModuleId = string;

interface Chunk {}

interface Exports {
  __esModule?: boolean;

  [key: string]: any;
}

export type ChunkModule = () => void;
export type ChunkRegistration = [
  chunkPath: ChunkPath,
  chunkModules: ChunkModule[],
  DevRuntimeParams | undefined
];

interface Module {
  exports: Exports;
  error: Error | undefined;
  loaded: boolean;
  id: ModuleId;
  hot?: Hot;
  children: ModuleId[];
  parents: ModuleId[];
  interopNamespace?: EsmInteropNamespace;
}

enum SourceType {
  /**
   * The module was instantiated because it was included in an evaluated chunk's
   * runtime.
   */
  Runtime = 0,
  /**
   * The module was instantiated because a parent module imported it.
   */
  Parent = 1,
  /**
   * The module was instantiated because it was included in a chunk's hot module
   * update.
   */
  Update = 2,
}

type SourceInfo =
  | {
      type: SourceType.Runtime;
      chunkPath: ChunkPath;
    }
  | {
      type: SourceType.Parent;
      parentId: ModuleId;
    }
  | {
      type: SourceType.Update;
      parents?: ModuleId[];
    };

type ModuleCache = Record<ModuleId, Module>;

type CommonJsRequire = (moduleId: ModuleId) => Exports;

export type EsmInteropNamespace = Record<string, any>;
type EsmImport = (
  moduleId: ModuleId,
  allowExportDefault: boolean
) => EsmInteropNamespace;
type EsmExport = (exportGetters: Record<string, () => any>) => void;
type ExportValue = (value: any) => void;

type LoadChunk = (chunkPath: ChunkPath) => Promise<any> | undefined;

interface TurbopackContext {
  e: Module["exports"];
  r: CommonJsRequire;
  i: EsmImport;
  s: EsmExport;
  v: ExportValue;
  m: Module;
  c: ModuleCache;
  l: LoadChunk;
  p: Partial<NodeJS.Process> & Pick<NodeJS.Process, "env">;
}

type ModuleFactory = (
  this: Module["exports"],
  context: TurbopackContext
) => undefined;

// string encoding of a module factory (used in hmr updates)
type ModuleFactoryString = string;

interface RuntimeBackend {
  registerChunk: (chunkPath: ChunkPath, params?: DevRuntimeParams) => void;
  loadChunk: (chunkPath: ChunkPath, source: SourceInfo) => Promise<void>;
  reloadChunk?: (chunkPath: ChunkPath) => Promise<void>;
  unloadChunk?: (chunkPath: ChunkPath) => void;

  restart: () => void;
}

export type UpdateCallback = (update: ServerMessage) => void;
export type ChunkUpdateProvider = {
  push: (registration: [ChunkPath, UpdateCallback]) => void;
};

export interface TurbopackGlobals {
  // This is used by the Next.js integration test suite to notify it when HMR
  // updates have been completed.
  __NEXT_HMR_CB?: null | (() => void);
  TURBOPACK?: ChunkRegistration[];
  TURBOPACK_CHUNK_UPDATE_LISTENERS?:
    | ChunkUpdateProvider
    | [ChunkPath, UpdateCallback][];
}

export type GetFirstModuleChunk = (moduleId: ModuleId) => ChunkPath | null;
export type GetOrInstantiateRuntimeModule = (
  moduleId: ModuleId,
  chunkPath: ChunkPath
) => Module;
export type RegisterChunkListAndMarkAsRuntime = (
  chunkListPath: ChunkPath,
  chunkPaths: ChunkPath[]
) => void;

export interface Loader {
  promise: Promise<undefined>;
  onLoad: () => void;
}

export type ModuleEffect =
  | {
      type: "unaccepted";
      dependencyChain: ModuleId[];
    }
  | {
      type: "self-declined";
      dependencyChain: ModuleId[];
      moduleId: ModuleId;
    }
  | {
      type: "accepted";
      moduleId: ModuleId;
      outdatedModules: Set<ModuleId>;
    };

declare global {
  var TURBOPACK: ChunkRegistration[];
  var TURBOPACK_CHUNK_UPDATE_LISTENERS:
    | ChunkUpdateProvider
    | [ChunkPath, UpdateCallback][]
    | undefined;

  var $RefreshHelpers$: RefreshRuntimeGlobals["$RefreshHelpers$"];
  var $RefreshReg$: RefreshRuntimeGlobals["$RefreshReg$"];
  var $RefreshSig$: RefreshRuntimeGlobals["$RefreshSig$"];
  var $RefreshInterceptModuleExecution$: RefreshRuntimeGlobals["$RefreshInterceptModuleExecution$"];

  interface NodeModule {
    hot: Hot;
  }
}
