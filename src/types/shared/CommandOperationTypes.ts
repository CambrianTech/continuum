/**
 * Command Operation Types - Universal enums for type-safe command operations
 * 
 * Eliminates magic string values across all command types and daemon communication.
 * Ensures compile-time safety and prevents runtime errors from typos.
 */

// Remote Command Operations
export enum RemoteCommandType {
  SCREENSHOT = 'screenshot',
  JS_EXECUTE = 'js-execute',
  BROWSER_NAVIGATE = 'browser-navigate',
  DOM_QUERY = 'dom-query',
  FILE_UPLOAD = 'file-upload',
  FORM_SUBMIT = 'form-submit'
}

// Data Marshal Operations
export enum DataMarshalOperation {
  ENCODE = 'encode',
  DECODE = 'decode',
  CHAIN = 'chain',
  EXTRACT = 'extract'
}

// Data Marshal Encoding Types
export enum DataMarshalEncoding {
  BASE64 = 'base64',
  JSON = 'json',
  RAW = 'raw'
}

// Execution Targets
export enum ExecutionTarget {
  BROWSER = 'browser',
  PYTHON = 'python',
  REMOTE_CONTINUUM = 'remote-continuum',
  AI_PERSONA = 'ai-persona',
  LOCAL_DAEMON = 'local-daemon'
}

// Command Source Types for tracking
export enum CommandSource {
  REMOTE_COMMAND = 'remote-command',
  BROWSER_EXECUTION = 'browser-execution',
  CHAT_COMMAND = 'chat-command',
  CHAT_SYSTEM = 'chat-system',
  SCREENSHOT_COMMAND = 'screenshot-command',
  FILE_COMMAND = 'file-command',
  SESSION_MANAGER = 'session-manager',
  DAEMON_SYSTEM = 'daemon-system'
}

// Universal Command Operations - Single source of truth
export enum CommandOperation {
  // ChatRoom operations
  CREATE_ROOM = 'create_room',
  JOIN_ROOM = 'join_room',
  LEAVE_ROOM = 'leave_room',
  SEND_MESSAGE = 'send_message',
  GET_MESSAGES = 'get_messages',
  LIST_ROOMS = 'list_rooms',
  GET_ROOM_INFO = 'get_room_info',
  DELETE_ROOM = 'delete_room',
  
  // Session operations
  CREATE_SESSION = 'create_session',
  JOIN_SESSION = 'join_session',
  END_SESSION = 'end_session',
  LIST_SESSIONS = 'list_sessions',
  
  // File operations
  READ_FILE = 'read_file',
  WRITE_FILE = 'write_file',
  DELETE_FILE = 'delete_file',
  LIST_FILES = 'list_files',
  
  // Browser operations
  TAKE_SCREENSHOT = 'take_screenshot',
  EXECUTE_JAVASCRIPT = 'execute_javascript',
  NAVIGATE_TO = 'navigate_to',
  CLICK_ELEMENT = 'click_element',
  TYPE_TEXT = 'type_text'
}

// Type guards for runtime validation
export function isRemoteCommandType(command: string): command is RemoteCommandType {
  return Object.values(RemoteCommandType).includes(command as RemoteCommandType);
}

export function isDataMarshalOperation(operation: string): operation is DataMarshalOperation {
  return Object.values(DataMarshalOperation).includes(operation as DataMarshalOperation);
}

export function isDataMarshalEncoding(encoding: string): encoding is DataMarshalEncoding {
  return Object.values(DataMarshalEncoding).includes(encoding as DataMarshalEncoding);
}

export function isExecutionTarget(target: string): target is ExecutionTarget {
  return Object.values(ExecutionTarget).includes(target as ExecutionTarget);
}

export function isCommandSource(source: string): source is CommandSource {
  return Object.values(CommandSource).includes(source as CommandSource);
}

export function isCommandOperation(operation: string): operation is CommandOperation {
  return Object.values(CommandOperation).includes(operation as CommandOperation);
}

// Helper functions to get specific operation subsets
export function getChatRoomOperations(): CommandOperation[] {
  return [
    CommandOperation.CREATE_ROOM,
    CommandOperation.JOIN_ROOM,
    CommandOperation.LEAVE_ROOM,
    CommandOperation.SEND_MESSAGE,
    CommandOperation.GET_MESSAGES,
    CommandOperation.LIST_ROOMS,
    CommandOperation.GET_ROOM_INFO,
    CommandOperation.DELETE_ROOM
  ];
}

export function getSessionOperations(): CommandOperation[] {
  return [
    CommandOperation.CREATE_SESSION,
    CommandOperation.JOIN_SESSION,
    CommandOperation.END_SESSION,
    CommandOperation.LIST_SESSIONS
  ];
}

export function getBrowserOperations(): CommandOperation[] {
  return [
    CommandOperation.TAKE_SCREENSHOT,
    CommandOperation.EXECUTE_JAVASCRIPT,
    CommandOperation.NAVIGATE_TO,
    CommandOperation.CLICK_ELEMENT,
    CommandOperation.TYPE_TEXT
  ];
}

export function getFileOperations(): CommandOperation[] {
  return [
    CommandOperation.READ_FILE,
    CommandOperation.WRITE_FILE,
    CommandOperation.DELETE_FILE,
    CommandOperation.LIST_FILES
  ];
}

// Type Groups for cleaner union types
export type ChatRoomOperations = 
  | CommandOperation.CREATE_ROOM
  | CommandOperation.JOIN_ROOM
  | CommandOperation.LEAVE_ROOM
  | CommandOperation.SEND_MESSAGE
  | CommandOperation.GET_MESSAGES
  | CommandOperation.LIST_ROOMS
  | CommandOperation.GET_ROOM_INFO
  | CommandOperation.DELETE_ROOM;

export type SessionOperations = 
  | CommandOperation.CREATE_SESSION
  | CommandOperation.JOIN_SESSION
  | CommandOperation.END_SESSION
  | CommandOperation.LIST_SESSIONS;

export type FileOperations = 
  | CommandOperation.READ_FILE
  | CommandOperation.WRITE_FILE
  | CommandOperation.DELETE_FILE
  | CommandOperation.LIST_FILES;

export type BrowserOperations = 
  | CommandOperation.TAKE_SCREENSHOT
  | CommandOperation.EXECUTE_JAVASCRIPT
  | CommandOperation.NAVIGATE_TO
  | CommandOperation.CLICK_ELEMENT
  | CommandOperation.TYPE_TEXT;

// Helper interfaces using the enums
export interface TypedRemoteRequest {
  command: RemoteCommandType;
  params: any;
  sessionId?: string;
  timeout?: number;
  executionTarget: ExecutionTarget;
}

export interface TypedDataMarshalOptions {
  operation: DataMarshalOperation;
  data?: any;
  encoding?: DataMarshalEncoding;
  correlationId?: string;
  source?: CommandSource;
  destination?: CommandSource;
  metadata?: Record<string, any>;
}