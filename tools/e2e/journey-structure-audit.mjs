// SPDX-FileCopyrightText: 2026 Aerobag contributors
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  requireWebDependency,
  webWorkspaceDirectory,
} from "../../ui/web-app/scripts/web-workspace-require.mjs";

const REPO_ROOT = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const ts = requireWebDependency("typescript");

export { webWorkspaceDirectory };

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
  "click",
  "drag",
  "enterText",
  "hover",
  "injectRasterLoadFault",
  "inspectMapAt",
  "openPage",
  "openChooser",
  "performAction",
  "reload",
  "revealElement",
  "revealProjectionMatching",
  "reset",
  "resetApplicationData",
  "scanProjection",
  "selectOption",
  "submit",
  "requestSubmit",
  "zoom",
]);

const MUTATING_FUNCTIONS = new Set([
  "activateAndroidNode",
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
  "readAction",
  "readCurrentPage",
  "readElement",
  "readNavigationAction",
  "readProjection",
  "readOption",
  "readSessionRevision",
]);
const RAW_DRIVER_METHODS = new Set([
  "back",
  "activateMapInspection",
  "drag",
  "enterText",
  "hover",
  "injectRasterLoadFault",
  "performAction",
  "openChooser",
  "selectOption",
  "submit",
  "zoom",
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

function containsRawAndroidInput(node) {
  if (!ts.isCallExpression(node) || calledFunction(node) !== "adb") return false;
  const args = node.arguments[1];
  return Boolean(
    args &&
    ts.isArrayLiteralExpression(args) &&
    args.elements.some((element) =>
      ts.isStringLiteral(element) && element.text === "input"),
  );
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

function isJourneyDriverCall(node) {
  if (!ts.isCallExpression(node) || !ts.isPropertyAccessExpression(node.expression)) return false;
  const receiver = node.expression.expression.getText();
  return receiver === "driver" || receiver.endsWith(".driver");
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

function isTransitionActionCallback(node) {
  let current = node.parent;
  while (current) {
    if (
      (ts.isFunctionExpression(current) || ts.isArrowFunction(current)) &&
      ts.isPropertyAssignment(current.parent) &&
      ts.isIdentifier(current.parent.name) &&
      current.parent.name.text === "act"
    ) {
      const contract = current.parent.parent;
      const call = contract?.parent;
      if (!ts.isObjectLiteralExpression(contract) || !ts.isCallExpression(call)) return false;
      const called = calledFunction(call);
      const method = calledMethod(call);
      return called === "performTransition" || called === "nativeTransition" || called === "transition" ||
        method === "transition";
    }
    if (
      ts.isPropertyAssignment(current) &&
      ts.isIdentifier(current.name) &&
      current.name.text === "act"
    ) {
      const contract = current.parent;
      const call = contract?.parent;
      if (!ts.isObjectLiteralExpression(contract) || !ts.isCallExpression(call)) return false;
      const called = calledFunction(call);
      const method = calledMethod(call);
      return called === "performTransition" || called === "nativeTransition" || called === "transition" ||
        method === "transition";
    }
    if (
      ts.isFunctionDeclaration(current) ||
      ts.isMethodDeclaration(current) ||
      ts.isFunctionExpression(current) || ts.isArrowFunction(current)
    ) return false;
    current = current.parent;
  }
  return false;
}

function reportNativeActionEvidence(source, violations, callback) {
  if (!callback || (!ts.isArrowFunction(callback) && !ts.isFunctionExpression(callback))) return;
  const evidenceNames = new Set(callback.parameters.flatMap((parameter) =>
    ts.isIdentifier(parameter.name) ? [parameter.name.text] : []));
  if (callback.parameters.length === 0) {
    violations.push({
      ...sourceLocation(source, callback),
      message: "nativeTransition action must accept its observed readiness evidence",
    });
  }
  visit(callback.body, (child) => {
    const called = calledFunction(child);
    if (called === "tapTag" || called === "waitForNode") {
      violations.push({
        ...sourceLocation(source, child),
        message: `nativeTransition action must not rediscover UI state through ${called}`,
      });
    }
    if (called !== "tapNode" && called !== "activateAndroidNode") return;
    const evidence = child.arguments[1];
    if (!evidence || !evidenceNames.has(evidence.getText(source))) {
      violations.push({
        ...sourceLocation(source, child),
        message: `${called} must use the readiness evidence passed to act`,
      });
    }
  });
}

export function auditJourneyStructure(text, filename = "release-journey-implementations.mjs") {
  const source = ts.createSourceFile(filename, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.JS);
  const rawAndroidInputMustBeContracted = filename.endsWith("run-android-e2e-suite.mjs");
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
      if (
        MUTATING_DRIVER_METHODS.has(method) ||
        ["step", "transition", "action", "chooseOption"].includes(method)
      ) {
        mutatingFunctions.add(name);
      }
      if (containsDomMutation(node) || containsRawAndroidInput(node)) mutatingFunctions.add(name);
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
        ["step", "transition", "action", "chooseOption"].includes(method) ||
        containsDomMutation(child) ||
        containsRawAndroidInput(child) ||
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
    if (method === "step") {
      violations.push({
        ...sourceLocation(source, node),
        message: "generic journey steps are forbidden; use a semantic transition or a typed runtime phase",
      });
    }
    if (
      RAW_DRIVER_METHODS.has(method) &&
      isJourneyDriverCall(node) &&
      !isTransitionActionCallback(node)
    ) {
      violations.push({
        ...sourceLocation(source, node),
        message: method === "performAction"
          ? "performAction must be the single act phase of a semantic transition"
          : `${method} must be the act phase of a semantic transition`,
      });
    }
    if (
      rawAndroidInputMustBeContracted &&
      containsRawAndroidInput(node) &&
      !isTransitionActionCallback(node)
    ) {
      violations.push({
        ...sourceLocation(source, node),
        message: "raw adb input must be the act phase of a semantic transition",
      });
    }
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
    if (
      timeoutArgument !== undefined &&
      node.arguments[timeoutArgument] &&
      ts.isNumericLiteral(node.arguments[timeoutArgument])
    ) {
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
    if (
      called === "performTransition" || called === "nativeTransition" ||
      method === "transition" || method === "action" || method === "repeatableAction" ||
      method === "repeatAction" || method === "chooseOption"
    ) {
      const contract = node.arguments[
        called === "nativeTransition" ? 2
          : (method === "action" || method === "repeatableAction" || method === "repeatAction") ? 2
            : method === "chooseOption" ? 3
              : 1
      ];
      if (!contract || !ts.isObjectLiteralExpression(contract)) return;
      if (
        (method === "action" || method === "repeatableAction" ||
          method === "repeatAction" || method === "chooseOption") &&
        !contract.properties.some((property) =>
          ts.isPropertyAssignment(property) &&
          ts.isIdentifier(property.name) &&
          property.name.text === "complete")
      ) {
        violations.push({
          ...sourceLocation(source, contract),
          message: `${method} must declare a semantic completion condition`,
        });
      }
      for (const property of contract.properties) {
        if (!ts.isPropertyAssignment(property) || !ts.isIdentifier(property.name)) continue;
        if (
          (method === "action" || method === "repeatableAction" || method === "repeatAction") &&
          property.name.text === "ready"
        ) {
          violations.push({
            ...sourceLocation(source, property),
            message: "action readiness must come from driver.readAction(actionId)",
          });
        }
        if (property.name.text === "ready" || property.name.text === "complete") {
          reportMutations(property.initializer, `${called ?? method} ${property.name.text} callback`);
        } else if (called === "nativeTransition" && property.name.text === "act") {
          reportNativeActionEvidence(source, violations, property.initializer);
        } else if (
          property.name.text === "responseTimeoutMs" &&
          !isTimingClass(property.initializer, "userTransitionDeadlineMs")
        ) {
          violations.push({
            ...sourceLocation(source, property.initializer),
            message: "user transition responseTimeoutMs must use E2E_TIMING.userTransitionDeadlineMs",
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
  const violations = auditQualificationJourneys(REPO_ROOT);
  for (const violation of violations) {
    console.error(
      `${violation.filename}:${violation.line}:${violation.column}: ${violation.message}`,
    );
  }
  if (violations.length > 0) process.exitCode = 1;
}
