/**
 * JTAG Logging Module
 * 
 * Handles all logging functionality (log, warn, error, critical, trace, probe, test)
 */

import { JTAGLogEntry, JTAG_LOG_LEVELS, JTAGConfig } from '../JTAGTypes';

export class JTAGLogging {
  
  static log(
    component: string, 
    message: string, 
    data?: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message,
      data,
      type: JTAG_LOG_LEVELS.LOG
    };
    
    processLogEntry(entry);
  }

  static critical(
    component: string, 
    event: string, 
    data?: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message: `CRITICAL: ${event}`,
      data,
      type: JTAG_LOG_LEVELS.CRITICAL
    };
    
    processLogEntry(entry);
  }

  static warn(
    component: string, 
    message: string, 
    data?: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message,
      data,
      type: JTAG_LOG_LEVELS.WARN
    };
    
    processLogEntry(entry);
  }

  static error(
    component: string, 
    message: string, 
    data?: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message,
      data,
      type: JTAG_LOG_LEVELS.ERROR
    };
    
    processLogEntry(entry);
  }

  static trace(
    component: string, 
    functionName: string, 
    phase: 'ENTER' | 'EXIT', 
    data?: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message: `TRACE: ${functionName} ${phase}`,
      data,
      type: JTAG_LOG_LEVELS.TRACE
    };
    
    processLogEntry(entry);
  }

  static probe(
    component: string, 
    probeName: string, 
    state: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message: `PROBE: ${probeName}`,
      data: state,
      type: JTAG_LOG_LEVELS.PROBE
    };
    
    processLogEntry(entry);
  }

  static test(
    component: string, 
    testName: string, 
    data?: any,
    config: JTAGConfig,
    context: 'browser' | 'server',
    processLogEntry: (entry: JTAGLogEntry) => void
  ): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context,
      component,
      message: `TEST: ${testName}`,
      data,
      type: JTAG_LOG_LEVELS.TEST
    };
    
    processLogEntry(entry);
  }
}