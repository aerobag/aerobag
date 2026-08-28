// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const ts = require("../../ui/web-app/node_modules/typescript/lib/typescript.js");

export const AUDITED_JOURNEY_FILES = Object.freeze([
  "tools/e2e/release-journey-implementations.mjs",
  "tools/e2e/cloud-journey-peer.mjs",
  "tools/e2e/run-android-e2e-suite.mjs",
  "tools/e2e/run-android-chrome-livefeed-e2e.mjs",
  "tools/e2e/semantic-journey-driver.mjs",
  "tools/e2e/web-semantic-transport.mjs",
  "ui/web-app/scripts/nav-db-rollover-e2e.mjs",
]);

const MUTATING_DRIVER_METHODS = new Set([
  "back",
  "chooseOption",
  "click",
  "drag",
  "enterText",
  "hover",
  "inspectMapAt",
  "openPage",
  "performAction",
  "reload",
  "revealElement",
  "revealProjectionMatching",
  "reset",
  "resetApplicationData",
  "scanProjection",
  "submit",
  "requestSubmit",
  "zoom",
]);

const MUTATING_FUNCTIONS = new Set([
  "launchFreshAndroidApp",
  "nativeTransition",
  "pressKey",
  "scrollHorizontallyUntilTag",
  "scrollUntilTag",
  "scrollUntilTagPrefix",
  "setAndroidRotation",
  "swipe",
  "tapFirstPresentTag",
  "tapNode",
  "tapTag",
]);

const OBSERVATION_METHODS = new Set(["eventually"]);
const OBSERVATION_FUNCTIONS = new Set(["observeUntil", "waitFor"]);
const OBSERVATION_CALLBACK_ARGUMENTS = new Map([
  ["eventually", 1],
  ["observeUntil", 1],
  ["waitFor", 0],
]);
const READ_ONLY_DRIVER_METHODS = new Set([
  "findProjectionMatching",
  "readElement",
  "readProjection",
]);
const TIMEOUT_ARGUMENTS = new Map([
  ["waitFor", 1],
  ["waitForMapFollowProbe", 2],
  ["waitForNode", 2],
  ["tapTag", 2],
]);

const DOM_MUTATION_PATTERN = /(?:\.click\s*\(|\.requestSubmit\s*\(|\.dispatchEvent\s*\(|\.value\s*=)/;

function containsDomMutation(node) {
  if (!ts.isCallExpression(node)) return false;
  const method = calledMethod(node);
  if (method !== "evalValue" && method !== "evaluate") return false;
  const expression = node.arguments[0];
  return Boolean(expression && DOM_MUTATION_PATTERN.test(expression.getText()));
}

function calledMethod(node) {
  if (!ts.isCallExpression(node) || !ts.isPropertyAccessExpression(node.expression)) return null;
  return node.expression.name.text;
}

function calledFunction(node) {
  return ts.isCallExpression(node) && ts.isIdentifier(node.expression)
    ? node.expression.text
    : null;
}

function isTimingClass(node, name) {
  return ts.isPropertyAccessExpression(node) &&
    ts.isIdentifier(node.expression) &&
    node.expression.text === "E2E_TIMING" &&
    node.name.text === name;
}

function visit(node, callback) {
  callback(node);
  ts.forEachChild(node, (child) => visit(child, callback));
}

function functionName(node) {
  if (ts.isFunctionDeclaration(node) && node.name) return node.name.text;
  if (
    (ts.isArrowFunction(node) || ts.isFunctionExpression(node)) &&
    ts.isVariableDeclaration(node.parent) &&
    ts.isIdentifier(node.parent.name)
  ) {
    return node.parent.name.text;
  }
  return null;
}

function sourceLocation(source, node) {
  const location = source.getLineAndCharacterOfPosition(node.getStart(source));
  return { line: location.line + 1, column: location.character + 1 };
}

export function auditJourneyStructure(text, filename = "release-journey-implementations.mjs") {
  const source = ts.createSourceFile(filename, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const functions = new Map();
  visit(source, (node) => {
    const name = functionName(node);
    if (name) functions.set(name, node);
  });

  const mutatingFunctions = new Set();
  const callsByFunction = new Map();
  for (const [name, fn] of functions) {
    const calls = new Set();
    visit(fn.body, (node) => {
      const method = calledMethod(node);
      if (MUTATING_DRIVER_METHODS.has(method) || method === "step" || method === "transition") {
        mutatingFunctions.add(name);
      }
      if (containsDomMutation(node)) mutatingFunctions.add(name);
      const called = calledFunction(node);
      if (called && MUTATING_FUNCTIONS.has(called)) mutatingFunctions.add(name);
      if (called) calls.add(called);
    });
    callsByFunction.set(name, calls);
  }
  let changed = true;
  while (changed) {
    changed = false;
    for (const [name, calls] of callsByFunction) {
      if (mutatingFunctions.has(name)) continue;
      if ([...calls].some((called) => mutatingFunctions.has(called))) {
        mutatingFunctions.add(name);
        changed = true;
      }
    }
  }

  const violations = [];
  function reportMutations(callback, label) {
    if (!callback || (
      !ts.isArrowFunction(callback) &&
      !ts.isFunctionExpression(callback) &&
      !ts.isMethodDeclaration(callback)
    )) return;
    visit(callback.body, (child) => {
      const method = calledMethod(child);
      const called = calledFunction(child);
      if (
        MUTATING_DRIVER_METHODS.has(method) ||
        method === "step" ||
        method === "transition" ||
        containsDomMutation(child) ||
        (called && (MUTATING_FUNCTIONS.has(called) || mutatingFunctions.has(called)))
      ) {
        violations.push({
          ...sourceLocation(source, child),
          message: `${label} invokes mutating operation ${method ?? called}`,
        });
      }
    });
  }

  visit(source, (node) => {
    if (
      ts.isMethodDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      READ_ONLY_DRIVER_METHODS.has(node.name.text)
    ) {
      reportMutations(node, `read-only driver method ${node.name.text}`);
    }
    if (!ts.isCallExpression(node)) return;
    const method = calledMethod(node);
    const called = calledFunction(node);
    const isObservation = OBSERVATION_METHODS.has(method) || OBSERVATION_FUNCTIONS.has(called);
    if (method === "eventually" && node.arguments[2] && ts.isNumericLiteral(node.arguments[2])) {
      violations.push({
        ...sourceLocation(source, node.arguments[2]),
        message: "raw eventually deadline is forbidden; use a named E2E_TIMING class",
      });
    }
    if (method === "enterText") {
      const options = node.arguments[2];
      if (options && ts.isObjectLiteralExpression(options) && options.properties.some((property) =>
        ts.isPropertyAssignment(property) &&
        ts.isIdentifier(property.name) &&
        property.name.text === "submit" &&
        property.initializer.kind === ts.SyntaxKind.TrueKeyword)) {
        violations.push({
          ...sourceLocation(source, node),
          message: "enterText must not also submit; model editing and submission as separate user actions",
        });
      }
    }
    const timeoutArgument = called ? TIMEOUT_ARGUMENTS.get(called) : undefined;
    if (timeoutArgument !== undefined && ts.isNumericLiteral(node.arguments[timeoutArgument])) {
      violations.push({
        ...sourceLocation(source, node.arguments[timeoutArgument]),
        message: `raw ${called} deadline is forbidden; use a named E2E_TIMING class`,
      });
    }
    if (isObservation) {
      const observationName = method ?? called;
      reportMutations(
        node.arguments[OBSERVATION_CALLBACK_ARGUMENTS.get(observationName)],
        `${observationName} callback`,
      );
    }
    if (called === "performTransition" || called === "nativeTransition" || method === "transition") {
      const contract = node.arguments[called === "nativeTransition" ? 2 : method === "transition" ? 1 : 1];
      if (!contract || !ts.isObjectLiteralExpression(contract)) return;
      for (const property of contract.properties) {
        if (!ts.isPropertyAssignment(property) || !ts.isIdentifier(property.name)) continue;
        if (property.name.text === "ready" || property.name.text === "complete") {
          reportMutations(property.initializer, `${called ?? method} ${property.name.text} callback`);
        } else if (
          property.name.text === "responseTimeoutMs" &&
          !isTimingClass(property.initializer, "userResponseMs")
        ) {
          violations.push({
            ...sourceLocation(source, property.initializer),
            message: "user transition responseTimeoutMs must use E2E_TIMING.userResponseMs",
          });
        }
      }
    }
  });

  for (const [name, fn] of functions) {
    visit(fn.body, (node) => {
      if (!["delay", "sleep"].includes(calledFunction(node))) return;
      violations.push({
        ...sourceLocation(source, node),
        message: `fixed delay is forbidden in journey function ${name}`,
      });
    });
  }
  return violations;
}

export function auditQualificationJourneys(repoRoot) {
  return AUDITED_JOURNEY_FILES.flatMap((relativePath) => {
    const filename = resolve(repoRoot, relativePath);
    return auditJourneyStructure(readFileSync(filename, "utf8"), filename)
      .map((violation) => ({ ...violation, filename: relativePath }));
  });
}

const invokedAsScript = process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (invokedAsScript) {
  const repoRoot = resolve(fileURLToPath(new URL("../..", import.meta.url)));
  const violations = auditQualificationJourneys(repoRoot);
  for (const violation of violations) {
    console.error(
      `${violation.filename}:${violation.line}:${violation.column}: ${violation.message}`,
    );
  }
  if (violations.length > 0) process.exitCode = 1;
}
