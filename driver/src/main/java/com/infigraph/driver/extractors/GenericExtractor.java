package com.infigraph.driver.extractors;

import org.antlr.v4.runtime.*;
import org.antlr.v4.runtime.tree.*;

import java.util.*;

public class GenericExtractor extends BaseExtractor {

    public static class SymbolMapping {
        public String rule;
        public String kind;
        public String nameFrom;
        public String[] namePath;
        public int nameIndex = -1;
        public boolean scope;
        public String split;
        public boolean formQualified;
    }

    public static class RelationMapping {
        public String rule;
        public String kind;
        public String targetFrom;
        public String[] targetPath;
        public int targetIndex = -1;
        public String targetFallback;
    }

    private final Map<String, List<SymbolMapping>> symbolRules = new HashMap<>();
    private final Map<String, List<RelationMapping>> relationRules = new HashMap<>();
    private boolean scanFormNames;

    public void addSymbolMapping(SymbolMapping m) {
        symbolRules.computeIfAbsent(m.rule, k -> new ArrayList<>()).add(m);
    }

    public void addRelationMapping(RelationMapping m) {
        relationRules.computeIfAbsent(m.rule, k -> new ArrayList<>()).add(m);
    }

    public void setScanFormNames(boolean v) {
        this.scanFormNames = v;
    }

    @Override
    public void init(ExtractContext ctx, String source) {
        if (scanFormNames) {
            parseFormNames(source, ctx.formNames);
        }
    }

    @Override
    protected boolean processRule(String ruleName, ParseTree tree,
            CommonTokenStream tokens, ExtractContext ctx) {

        boolean isScope = false;

        List<SymbolMapping> symMappings = symbolRules.get(ruleName);
        if (symMappings != null) {
            for (SymbolMapping m : symMappings) {
                String name = resolveName(tree, m.nameFrom, m.namePath, m.nameIndex, ctx.ruleNames);
                if (name == null) continue;

                int[] span = getSpan((RuleContext) tree, tokens);

                if (m.split != null && !m.split.isEmpty()) {
                    for (String part : name.split(m.split)) {
                        part = part.trim();
                        if (!part.isEmpty()) {
                            ctx.pushSymbol(part, m.kind,
                                span[0], span[1], span[2], span[3],
                                collectRawText(tree), m.formQualified);
                        }
                    }
                } else {
                    ctx.pushSymbol(name, m.kind,
                        span[0], span[1], span[2], span[3],
                        collectRawText(tree), m.formQualified);
                }

                if (m.scope) {
                    ctx.scopeStack.push(name);
                    isScope = true;
                }
            }
        }

        List<RelationMapping> relMappings = relationRules.get(ruleName);
        if (relMappings != null) {
            for (RelationMapping m : relMappings) {
                String target = resolveName(tree, m.targetFrom, m.targetPath, m.targetIndex, ctx.ruleNames);
                if (target == null && m.targetFallback != null) {
                    target = resolveName(tree, m.targetFallback, null, -1, ctx.ruleNames);
                }
                if (target != null) {
                    int[] span = getSpan((RuleContext) tree, tokens);
                    ctx.pushRelation(target, m.kind,
                        span[0], span[1], span[2], span[3]);
                }
            }
        }

        return isScope;
    }

    private String resolveName(ParseTree tree, String from, String[] path, int index, String[] ruleNames) {
        if (path != null && path.length > 0) {
            ParseTree current = tree;
            for (int i = 0; i < path.length - 1; i++) {
                current = findChildNode(current, path[i], ruleNames);
                if (current == null) return null;
            }
            String lastStep = path[path.length - 1];
            return findChildRawText(current, lastStep, ruleNames);
        }

        if (from != null) {
            if (index >= 0) {
                return findChildRawTextByIndex(tree, from, index, ruleNames);
            }
            return findChildRawText(tree, from, ruleNames);
        }

        return null;
    }

    private ParseTree findChildNode(ParseTree tree, String childRule, String[] ruleNames) {
        for (int i = 0; i < tree.getChildCount(); i++) {
            ParseTree child = tree.getChild(i);
            if (child instanceof RuleContext) {
                int idx = ((RuleContext) child).getRuleIndex();
                if (idx >= 0 && idx < ruleNames.length && ruleNames[idx].equals(childRule)) {
                    return child;
                }
            }
        }
        return null;
    }
}
