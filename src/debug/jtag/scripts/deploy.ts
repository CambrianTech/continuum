#!/usr/bin/env npx tsx

/**
 * Deploy JTAG test-bench with clean installation
 * 
 * This script handles the complete deployment process:
 * 1. Updates test-bench package.json to use latest JTAG tarball
 * 2. Cleans node_modules and package-lock.json to avoid conflicts
 * 3. Performs fresh npm install
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

interface PackageJson {
  version: string;
  dependencies?: Record<string, string>;
  [key: string]: any;
}

interface DeploymentInfo {
  version: string;
  tarballName: string;
  tarballPath: string;
}

interface ProjectInfo {
  name: string;
  projectDir: string;
  packageJsonPath: string;
}

function getCurrentVersion(): DeploymentInfo {
  const mainPackagePath = path.join(__dirname, '..', 'package.json');
  const mainPackage: PackageJson = JSON.parse(fs.readFileSync(mainPackagePath, 'utf8'));
  const version = mainPackage.version;
  const tarballName = `continuum-jtag-${version}.tgz`;
  const tarballPath = path.join(__dirname, '..', tarballName);
  
  if (!fs.existsSync(tarballPath)) {
    throw new Error(`Tarball not found: ${tarballPath}`);
  }
  
  return { version, tarballName, tarballPath };
}

function discoverNodeProjects(): ProjectInfo[] {
  const examplesDir = path.join(__dirname, '..', 'examples');
  const projects: ProjectInfo[] = [];
  
  const items = fs.readdirSync(examplesDir, { withFileTypes: true });
  
  for (const item of items) {
    if (item.isDirectory()) {
      const projectDir = path.join(examplesDir, item.name);
      const packageJsonPath = path.join(projectDir, 'package.json');
      
      if (fs.existsSync(packageJsonPath)) {
        projects.push({
          name: item.name,
          projectDir,
          packageJsonPath
        });
      }
    }
  }
  
  return projects;
}

function cleanProjectDependencies(projectDir: string): void {
  const nodeModulesPath = path.join(projectDir, 'node_modules');
  const packageLockPath = path.join(projectDir, 'package-lock.json');
  
  if (fs.existsSync(nodeModulesPath)) {
    fs.rmSync(nodeModulesPath, { recursive: true, force: true });
    console.log(`   🗑️  Removed node_modules`);
  }
  
  if (fs.existsSync(packageLockPath)) {
    fs.unlinkSync(packageLockPath);
    console.log(`   🗑️  Removed package-lock.json`);
  }
}

function deployToSingleProject(project: ProjectInfo, deployment: DeploymentInfo): void {
  console.log(`\n🚀 Deploying to ${project.name}...`);
  
  // Read and update package.json
  const projectPackage: PackageJson = JSON.parse(fs.readFileSync(project.packageJsonPath, 'utf8'));
  
  if (!projectPackage.dependencies) {
    projectPackage.dependencies = {};
  }
  
  projectPackage.dependencies['@continuum/jtag'] = `file:../../${deployment.tarballName}`;
  
  // Write back the updated package.json
  fs.writeFileSync(project.packageJsonPath, JSON.stringify(projectPackage, null, 2) + '\n');
  
  console.log(`   ✅ Updated ${project.name} to use JTAG v${deployment.version}`);
  console.log(`   📋 Dependency: @continuum/jtag → file:../../${deployment.tarballName}`);
  
  // Clean dependencies
  console.log(`   🧹 Cleaning dependencies...`);
  cleanProjectDependencies(project.projectDir);
  
  // Fresh npm install
  console.log(`   📥 Installing dependencies...`);
  execSync('npm install', { 
    cwd: project.projectDir, 
    stdio: 'inherit' 
  });
  
  console.log(`   ✅ ${project.name} deployment complete!`);
}

function deployToNode(): void {
  try {
    const deployment = getCurrentVersion();
    console.log(`📦 Current JTAG version: ${deployment.version}`);
    console.log(`✅ Found tarball: ${deployment.tarballName}`);
    
    const projects = discoverNodeProjects();
    
    if (projects.length === 0) {
      console.log(`ℹ️  No Node.js projects found in examples/`);
      return;
    }
    
    console.log(`🔍 Found ${projects.length} Node.js project(s): ${projects.map(p => p.name).join(', ')}`);
    
    for (const project of projects) {
      deployToSingleProject(project, deployment);
    }
    
    console.log(`\n🎉 All Node.js projects deployed successfully!`);
    
  } catch (error) {
    console.error(`❌ Failed to deploy to Node.js projects: ${error}`);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
    deployToNode();
}

export { deployToNode };