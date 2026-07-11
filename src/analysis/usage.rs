use super::walker::WalkDir;
use crate::amber_anyhow::{Context, Result};
use proc_macro2::Span;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::Visit;
use syn::{
    parse_file, Expr, ExprMacro, ExprMethodCall, ExprPath, ImplItemFn, ItemEnum, ItemFn,
    ItemStruct, ItemTrait, ItemType, ItemUse, Type, TypePath,
};
use tracing::{info, trace};

use super::types::{
    CallSite, CrateUsage, Dependency, DependencyKind, DependencySource, ImportedItem, ItemKind,
    Location, UsageKind,
};

/// Analyzes how dependencies are actually used in source code
pub struct UsageAnalyzer {
    source_root: PathBuf,
    manifest_dir: PathBuf,
}

impl UsageAnalyzer {
    /// Create a new usage analyzer for the project at `manifest_path`.
    ///
    /// # Errors
    ///
    /// Returns an error if `manifest_path` has no parent directory.
    pub fn new(manifest_path: &Path) -> Result<Self> {
        let manifest_dir = manifest_path
            .parent()
            .context("Invalid manifest path")?
            .to_path_buf();

        let source_root = manifest_dir.join("src");

        Ok(Self {
            source_root,
            manifest_dir,
        })
    }

    /// Analyze usage for all dependencies.
    ///
    /// # Errors
    ///
    /// Returns an error if source files cannot be scanned.
    pub fn analyze_all_usage(&self, deps: &[Dependency]) -> Result<HashMap<String, CrateUsage>> {
        info!("Scanning source files for dependency usage...");

        let mut all_usage: HashMap<String, CrateUsage> = HashMap::new();

        // Initialize usage records for all deps
        for dep in deps {
            all_usage.insert(
                dep.name.clone(),
                CrateUsage {
                    crate_name: dep.name.clone(),
                    ..Default::default()
                },
            );
        }

        // Walk all Rust source files
        let rust_files = self.find_rust_files();
        info!("Found {} Rust source files to analyze", rust_files.len());

        for file_path in &rust_files {
            trace!("Analyzing {}", file_path.display());
            if let Ok(content) = fs::read_to_string(file_path) {
                let relative_path = file_path
                    .strip_prefix(&self.manifest_dir)
                    .unwrap_or(file_path)
                    .to_string_lossy()
                    .to_string();

                Self::analyze_file_usage(&content, deps, &mut all_usage, &relative_path);
            }
        }

        let dep_api_counts: HashMap<String, usize> = deps
            .iter()
            .map(|dep| (dep.name.clone(), dep.public_api_count))
            .collect();
        let dep_loc_approx: HashMap<String, usize> = deps
            .iter()
            .map(|dep| (dep.name.clone(), dep.loc_approx))
            .collect();

        // Post-process: determine trivial usage, calculate coverage
        for usage in all_usage.values_mut() {
            usage.unique_api_usage = usage
                .imported_items
                .iter()
                .map(|i| &i.name)
                .collect::<HashSet<_>>()
                .len();

            usage.api_coverage_percent = if usage.imported_items.is_empty() {
                0.0
            } else {
                // Prefer the crate's declared public API count. For crates where
                // it is unavailable, fall back to a LOC-based heuristic with a
                // floor so tiny crates cannot report >100% coverage.
                let estimated_total_apis = dep_api_counts
                    .get(&usage.crate_name)
                    .copied()
                    .filter(|&count| count > 0)
                    .map_or_else(
                        || {
                            let loc = dep_loc_approx.get(&usage.crate_name).copied().unwrap_or(0);
                            #[allow(clippy::cast_precision_loss)]
                            {
                                (loc / 100).clamp(5, 200) as f64
                            }
                        },
                        |count| {
                            #[allow(clippy::cast_precision_loss)]
                            {
                                count as f64
                            }
                        },
                    );
                #[allow(clippy::cast_precision_loss)]
                let percent =
                    ((usage.unique_api_usage as f64 / estimated_total_apis) * 100.0).min(100.0);
                percent
            };

            usage.is_trivial_usage = usage.call_sites.len() < 5 && usage.unique_api_usage <= 3;

            usage.import_count = usage.imported_items.len();
        }

        let used_count = all_usage
            .values()
            .filter(|u| !u.imported_items.is_empty() || !u.call_sites.is_empty())
            .count();
        info!(
            "Usage analysis complete: {} of {} dependencies have active usage",
            used_count,
            deps.len()
        );

        Ok(all_usage)
    }

    /// Analyze usage for a specific crate.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest directory is invalid or source files
    /// cannot be scanned.
    pub fn analyze_crate_usage(&self, crate_name: &str) -> Result<CrateUsage> {
        let fake_dep = Dependency {
            name: crate_name.to_string(),
            version: String::new(),
            source: DependencySource::CratesIo,
            kind: DependencyKind::Normal,
            features: Vec::new(),
            optional: false,
            uses_default_features: true,
            transitive_deps: Vec::new(),
            loc_approx: 0,
            public_api_count: 0,
            last_release: None,
            maintenance_score: 50,
            cve_count: 0,
            license: None,
            download_count: 0,
        };

        let mut all_usage = self.analyze_all_usage(&[fake_dep])?;
        Ok(all_usage.remove(crate_name).unwrap_or_default())
    }

    fn find_rust_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();

        if self.source_root.exists() {
            for entry in WalkDir::new(&self.source_root)
                .into_iter()
                .filter_map(std::result::Result::ok)
            {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "rs") {
                    files.push(path.to_path_buf());
                }
            }
        }

        // Also check examples, tests, benches
        for dir in &["examples", "tests", "benches"] {
            let dir_path = self.manifest_dir.join(dir);
            if dir_path.exists() {
                for entry in WalkDir::new(&dir_path)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "rs") {
                        files.push(path.to_path_buf());
                    }
                }
            }
        }

        files
    }

    fn analyze_file_usage(
        content: &str,
        deps: &[Dependency],
        all_usage: &mut HashMap<String, CrateUsage>,
        file_path: &str,
    ) {
        let Ok(ast) = parse_file(content) else {
            return;
        };

        let dep_names: HashSet<String> = deps.iter().map(|d| d.name.clone()).collect();

        let mut visitor = UsageVisitor {
            dep_names: &dep_names,
            alias_map: HashMap::new(),
            imported_name_map: HashMap::new(),
            all_usage,
            current_file: file_path,
            in_public_api: false,
        };

        // First pass: collect aliases from use statements
        AliasCollector {
            dep_names: &dep_names,
            alias_map: &mut visitor.alias_map,
        }
        .visit_file(&ast);

        // Second pass: collect all usages (imports populate imported_name_map)
        visitor.visit_file(&ast);
    }
}

struct AliasCollector<'a> {
    dep_names: &'a HashSet<String>,
    alias_map: &'a mut HashMap<String, String>,
}

impl<'ast> Visit<'ast> for AliasCollector<'_> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        collect_aliases(&node.tree, self.dep_names, self.alias_map);
        syn::visit::visit_item_use(self, node);
    }
}

fn collect_aliases(
    tree: &syn::UseTree,
    dep_names: &HashSet<String>,
    alias_map: &mut HashMap<String, String>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            let first = path.ident.to_string();
            if dep_names.contains(&first) {
                // potential alias deeper in the tree
                collect_aliases(&path.tree, dep_names, alias_map);
            }
        }
        syn::UseTree::Rename(rename) => {
            let original = rename.ident.to_string();
            if dep_names.contains(&original) {
                alias_map.insert(rename.rename.to_string(), original);
            }
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_aliases(item, dep_names, alias_map);
            }
        }
        _ => {}
    }
}

struct UsageVisitor<'a> {
    dep_names: &'a HashSet<String>,
    alias_map: HashMap<String, String>,
    imported_name_map: HashMap<String, String>,
    all_usage: &'a mut HashMap<String, CrateUsage>,
    current_file: &'a str,
    in_public_api: bool,
}

impl UsageVisitor<'_> {
    fn resolve_crate(&self, first_segment: &str) -> Option<String> {
        if self.dep_names.contains(first_segment) {
            Some(first_segment.to_string())
        } else {
            self.alias_map
                .get(first_segment)
                .map(std::string::ToString::to_string)
        }
    }

    fn record_import(
        &mut self,
        crate_name: &str,
        name: &str,
        kind: ItemKind,
        path: &str,
        span: Span,
    ) {
        self.imported_name_map
            .insert(name.to_string(), crate_name.to_string());

        let usage = self
            .all_usage
            .entry(crate_name.to_string())
            .or_insert_with(|| CrateUsage {
                crate_name: crate_name.to_string(),
                ..Default::default()
            });

        usage.imported_items.push(ImportedItem {
            name: name.to_string(),
            kind,
            path: path.to_string(),
            location: location_from_span(self.current_file, span),
        });

        if self.in_public_api {
            usage.used_in_public_api = true;
        }

        if !usage
            .affected_files
            .contains(&self.current_file.to_string())
        {
            usage.affected_files.push(self.current_file.to_string());
        }
    }

    fn record_call_site(
        &mut self,
        crate_name: &str,
        function_name: &str,
        kind: UsageKind,
        span: Span,
        context: &str,
    ) {
        let usage = self
            .all_usage
            .entry(crate_name.to_string())
            .or_insert_with(|| CrateUsage {
                crate_name: crate_name.to_string(),
                ..Default::default()
            });

        usage.call_sites.push(CallSite {
            function_name: function_name.to_string(),
            kind,
            location: location_from_span(self.current_file, span),
            context: context.to_string(),
        });

        if self.in_public_api {
            usage.used_in_public_api = true;
        }

        if !usage
            .affected_files
            .contains(&self.current_file.to_string())
        {
            usage.affected_files.push(self.current_file.to_string());
        }
    }

    fn mark_public_api(&mut self, crate_name: &str) {
        if let Some(usage) = self.all_usage.get_mut(crate_name) {
            usage.used_in_public_api = true;
        }
    }

    fn with_public_api<F, R>(&mut self, is_public: bool, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let prev = self.in_public_api;
        self.in_public_api = is_public;
        let result = f(self);
        self.in_public_api = prev;
        result
    }

    fn check_impl_paths(&mut self, item_impl: &syn::ItemImpl) {
        if let Some((_, trait_path, _)) = &item_impl.trait_ {
            if let Some(first) = trait_path.segments.first() {
                let first_name = first.ident.to_string();
                if let Some(crate_name) = self.resolve_crate(&first_name) {
                    let name = trait_path
                        .segments
                        .last()
                        .map_or_else(|| first_name.clone(), |s| s.ident.to_string());
                    let path = path_to_string(trait_path);
                    self.record_call_site(
                        &crate_name,
                        &name,
                        UsageKind::TraitBound,
                        trait_path.span(),
                        &format!("impl trait: {path}"),
                    );
                    self.mark_public_api(&crate_name);
                }
            }
        }

        if let Type::Path(ty) = item_impl.self_ty.as_ref() {
            if let Some(first) = ty.path.segments.first() {
                let first_name = first.ident.to_string();
                if let Some(crate_name) = self.resolve_crate(&first_name) {
                    let name = ty
                        .path
                        .segments
                        .last()
                        .map_or_else(|| first_name.clone(), |s| s.ident.to_string());
                    let path = path_to_string(&ty.path);
                    self.record_call_site(
                        &crate_name,
                        &name,
                        UsageKind::TypeReference,
                        ty.span(),
                        &format!("impl self type: {path}"),
                    );
                    self.mark_public_api(&crate_name);
                }
            }
        }
    }

    fn check_expr_path(&mut self, expr: &ExprPath) {
        let first = match expr.path.segments.first() {
            Some(seg) => seg.ident.to_string(),
            None => return,
        };

        if let Some(crate_name) = self.resolve_crate(&first) {
            let name = expr
                .path
                .segments
                .last()
                .map_or_else(|| first.clone(), |s| s.ident.to_string());
            let path = path_to_string(&expr.path);
            self.record_call_site(
                &crate_name,
                &name,
                UsageKind::FunctionCall,
                expr.span(),
                &format!("path expression: {path}"),
            );
        }
    }

    fn check_type_path(&mut self, ty: &TypePath) {
        let first = match ty.path.segments.first() {
            Some(seg) => seg.ident.to_string(),
            None => return,
        };

        if let Some(crate_name) = self.resolve_crate(&first) {
            let name = ty
                .path
                .segments
                .last()
                .map_or_else(|| first.clone(), |s| s.ident.to_string());
            let path = path_to_string(&ty.path);
            self.record_call_site(
                &crate_name,
                &name,
                UsageKind::TypeReference,
                ty.span(),
                &format!("type reference: {path}"),
            );
        }
    }

    fn check_macro(&mut self, mac: &ExprMacro) {
        let first = match mac.mac.path.segments.first() {
            Some(seg) => seg.ident.to_string(),
            None => return,
        };

        if let Some(crate_name) = self.resolve_crate(&first) {
            let name = mac
                .mac
                .path
                .segments
                .last()
                .map_or_else(|| first.clone(), |s| s.ident.to_string());
            let path = path_to_string(&mac.mac.path);
            self.record_call_site(
                &crate_name,
                &name,
                UsageKind::MacroInvocation,
                mac.span(),
                &format!("macro invocation: {path}!"),
            );
        }
    }

    fn check_attribute(&mut self, attr: &syn::Attribute) {
        let first = match attr.path().segments.first() {
            Some(seg) => seg.ident.to_string(),
            None => return,
        };

        // Direct attribute from a tracked crate, e.g. #[serde(...)]
        if let Some(crate_name) = self.resolve_crate(&first) {
            let path = path_to_string(attr.path());
            self.record_call_site(
                &crate_name,
                &first,
                UsageKind::Attribute,
                attr.span(),
                &format!("attribute: #{path}"),
            );
            return;
        }

        // Attribute introduced via an import, e.g. use serde::Serialize; #[derive(Serialize)]
        if let Some(crate_name) = self.imported_name_map.get(&first).cloned() {
            self.record_call_site(
                &crate_name,
                &first,
                UsageKind::Attribute,
                attr.span(),
                &format!("attribute: #{first} (from {crate_name})"),
            );
            return;
        }

        // Derive attributes may reference imported macros by name.
        if first == "derive" {
            self.check_derive_attribute(attr);
        }
    }

    fn check_derive_attribute(&mut self, attr: &syn::Attribute) {
        let tokens: std::result::Result<
            syn::punctuated::Punctuated<syn::Path, syn::Token![,]>,
            syn::Error,
        > = attr.parse_args_with(syn::punctuated::Punctuated::parse_terminated);

        if let Ok(paths) = tokens {
            for path in paths {
                let first_seg = path
                    .segments
                    .first()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();

                if let Some(crate_name) = self.imported_name_map.get(&first_seg).cloned() {
                    let name = path
                        .segments
                        .last()
                        .map_or_else(|| first_seg.clone(), |s| s.ident.to_string());
                    self.record_call_site(
                        &crate_name,
                        &name,
                        UsageKind::Attribute,
                        attr.span(),
                        &format!(
                            "derive macro: {} (from {crate_name})",
                            path_to_string(&path)
                        ),
                    );
                }
            }
        }
    }

    fn check_stmt_macro(&mut self, stmt_macro: &syn::StmtMacro) {
        let first = match stmt_macro.mac.path.segments.first() {
            Some(seg) => seg.ident.to_string(),
            None => return,
        };

        if let Some(crate_name) = self.resolve_crate(&first) {
            let name = stmt_macro
                .mac
                .path
                .segments
                .last()
                .map_or_else(|| first.clone(), |s| s.ident.to_string());
            let path = path_to_string(&stmt_macro.mac.path);
            self.record_call_site(
                &crate_name,
                &name,
                UsageKind::MacroInvocation,
                stmt_macro.span(),
                &format!("macro invocation: {path}!"),
            );
        }
    }
}

impl<'ast> Visit<'ast> for UsageVisitor<'_> {
    fn visit_item_use(&mut self, node: &'ast ItemUse) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        let prev = self.in_public_api;
        self.in_public_api = is_public;
        collect_imports(&node.tree, self);
        syn::visit::visit_item_use(self, node);
        self.in_public_api = prev;
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        self.with_public_api(is_public, |slf| {
            syn::visit::visit_item_fn(slf, node);
        });
    }

    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        self.with_public_api(is_public, |slf| {
            syn::visit::visit_item_struct(slf, node);
        });
    }

    fn visit_item_enum(&mut self, node: &'ast ItemEnum) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        self.with_public_api(is_public, |slf| {
            syn::visit::visit_item_enum(slf, node);
        });
    }

    fn visit_item_trait(&mut self, node: &'ast ItemTrait) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        self.with_public_api(is_public, |slf| {
            syn::visit::visit_item_trait(slf, node);
        });
    }

    fn visit_item_type(&mut self, node: &'ast ItemType) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        self.with_public_api(is_public, |slf| {
            syn::visit::visit_item_type(slf, node);
        });
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        self.check_impl_paths(node);
        syn::visit::visit_item_impl(self, node);
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let is_public = matches!(node.vis, syn::Visibility::Public(_));
        self.with_public_api(is_public, |slf| {
            syn::visit::visit_impl_item_fn(slf, node);
        });
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        match node {
            Expr::Path(expr) => self.check_expr_path(expr),
            Expr::Call(call) => {
                if let Expr::Path(func) = &*call.func {
                    self.check_expr_path(func);
                }
            }
            Expr::MethodCall(ExprMethodCall {
                receiver,
                method,
                args,
                ..
            }) => {
                // Heuristic attribution for method calls. We cannot resolve the
                // receiver type without a full type-check, but two common cases
                // are strong enough signals to record:
                //
                // 1. The receiver is a path starting with a tracked crate name or
                //    alias (e.g. `crate::Type::method()` or `alias.method()`).
                // 2. The method name matches an imported item from a tracked crate
                //    (e.g. a trait or free function brought into scope).
                if let Expr::Path(expr_path) = receiver.as_ref() {
                    if let Some(first) = expr_path.path.segments.first() {
                        let first = first.ident.to_string();
                        let crate_name = self
                            .resolve_crate(&first)
                            .or_else(|| self.imported_name_map.get(&first).cloned());
                        if let Some(crate_name) = crate_name {
                            let path = path_to_string(&expr_path.path);
                            self.record_call_site(
                                &crate_name,
                                &method.to_string(),
                                UsageKind::MethodCall,
                                method.span(),
                                &format!("method call: {path}::{method}({} args)", args.len()),
                            );
                        }
                    }
                }

                let method_name = method.to_string();
                if let Some(crate_name) = self.imported_name_map.get(&method_name).cloned() {
                    self.record_call_site(
                        &crate_name,
                        &method_name,
                        UsageKind::MethodCall,
                        method.span(),
                        &format!("method call attributed to imported {method_name}"),
                    );
                }
            }
            Expr::Macro(mac) => self.check_macro(mac),
            _ => {}
        }
        syn::visit::visit_expr(self, node);
    }

    fn visit_type(&mut self, node: &'ast Type) {
        if let Type::Path(ty) = node {
            self.check_type_path(ty);
        }
        syn::visit::visit_type(self, node);
    }

    fn visit_attribute(&mut self, node: &'ast syn::Attribute) {
        self.check_attribute(node);
        syn::visit::visit_attribute(self, node);
    }

    fn visit_stmt(&mut self, node: &'ast syn::Stmt) {
        match node {
            syn::Stmt::Expr(Expr::Macro(mac), _) => {
                self.check_macro(mac);
            }
            syn::Stmt::Macro(stmt_macro) => self.check_stmt_macro(stmt_macro),
            _ => {}
        }
        syn::visit::visit_stmt(self, node);
    }
}

fn location_from_span(file: &str, span: Span) -> Location {
    let start = span.start();
    Location::new(file, start.line, start.column)
}

fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn collect_imports(tree: &syn::UseTree, visitor: &mut UsageVisitor<'_>) {
    if let syn::UseTree::Path(path) = tree {
        let first = path.ident.to_string();
        if visitor.dep_names.contains(&first) || visitor.alias_map.contains_key(&first) {
            let crate_name = visitor
                .resolve_crate(&first)
                .unwrap_or_else(|| first.clone());
            collect_imports_inner(&path.tree, &crate_name, &first, visitor);
        }
    }
}

fn collect_imports_inner(
    tree: &syn::UseTree,
    crate_name: &str,
    prefix: &str,
    visitor: &mut UsageVisitor<'_>,
) {
    match tree {
        syn::UseTree::Path(path) => {
            let new_prefix = format!("{}::{}", prefix, path.ident);
            collect_imports_inner(&path.tree, crate_name, &new_prefix, visitor);
        }
        syn::UseTree::Name(name) => {
            let item_name = name.ident.to_string();
            let kind = guess_item_kind(&item_name);
            visitor.record_import(
                crate_name,
                &item_name,
                kind,
                &format!("{prefix}::{item_name}"),
                name.span(),
            );
        }
        syn::UseTree::Rename(rename) => {
            let item_name = rename.rename.to_string();
            let kind = guess_item_kind(&item_name);
            visitor.record_import(
                crate_name,
                &item_name,
                kind,
                &format!("{}::{} as {}", prefix, rename.ident, rename.rename),
                rename.span(),
            );
        }
        syn::UseTree::Glob(glob) => {
            visitor.record_import(
                crate_name,
                "*",
                ItemKind::Module,
                &format!("{prefix}::*"),
                glob.span(),
            );
        }
        syn::UseTree::Group(group) => {
            for item in &group.items {
                collect_imports_inner(item, crate_name, prefix, visitor);
            }
        }
    }
}

fn guess_item_kind(name: &str) -> ItemKind {
    if name.ends_with('!') || name.starts_with("macro_") {
        ItemKind::Macro
    } else if name.starts_with("const_") || name.starts_with("CONST_") {
        ItemKind::Constant
    } else if name.chars().next().is_some_and(char::is_uppercase) {
        ItemKind::Type
    } else {
        ItemKind::Function
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guess_item_kind_classifies_names() {
        assert!(matches!(guess_item_kind("MyStruct"), ItemKind::Type));
        assert!(matches!(guess_item_kind("my_fn"), ItemKind::Function));
        assert!(matches!(guess_item_kind("CONST_FOO"), ItemKind::Constant));
    }

    fn dep_named(name: &str) -> Dependency {
        Dependency {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            source: DependencySource::CratesIo,
            kind: DependencyKind::Normal,
            features: Vec::new(),
            optional: false,
            uses_default_features: true,
            transitive_deps: Vec::new(),
            loc_approx: 0,
            public_api_count: 0,
            last_release: None,
            maintenance_score: 50,
            cve_count: 0,
            license: None,
            download_count: 0,
        }
    }

    #[test]
    fn detects_function_call() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn main() { my_crate::do_work(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::FunctionCall),
            "expected a function call"
        );
    }

    #[test]
    fn detects_renamed_import() {
        let deps = vec![dep_named("serde")];
        let mut usage = HashMap::new();
        let code = r"
            use serde as s;
            fn main() { s::Serialize; }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let serde = usage.get("serde").unwrap();
        assert!(
            serde
                .call_sites
                .iter()
                .any(|c| c.function_name == "Serialize"),
            "expected usage through alias"
        );
    }

    #[test]
    fn detects_glob_import() {
        let deps = vec![dep_named("log")];
        let mut usage = HashMap::new();
        let code = r"use log::*;";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let log = usage.get("log").unwrap();
        assert!(log.imported_items.iter().any(|i| i.name == "*"));
    }

    #[test]
    fn detects_derive_attribute() {
        let deps = vec![dep_named("serde")];
        let mut usage = HashMap::new();
        let code = r"
            use serde::Serialize;
            #[derive(Serialize)]
            struct Foo;
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let serde = usage.get("serde").unwrap();
        assert!(
            serde
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::Attribute),
            "expected derive attribute"
        );
    }

    #[test]
    fn detects_method_call_heuristic() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            use my_crate::Worker;
            fn demo() { Worker.process(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MethodCall),
            "expected method call attribution"
        );
    }

    #[test]
    fn detects_macro_invocation() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn main() { my_crate::foo!(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MacroInvocation),
            "expected macro invocation"
        );
    }

    #[test]
    fn detects_leading_colon_path() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn main() { ::my_crate::do_work(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::FunctionCall),
            "expected a function call through leading-colon path"
        );
    }

    #[test]
    fn detects_direct_attribute() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            #[my_crate::attr]
            fn demo() {}
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::Attribute),
            "expected direct attribute"
        );
    }

    #[test]
    fn detects_group_import_with_rename() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"use my_crate::{Foo, Bar as Baz};";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(my_crate.imported_items.iter().any(|i| i.name == "Foo"));
        assert!(my_crate.imported_items.iter().any(|i| i.name == "Baz"));
    }

    #[test]
    fn detects_type_reference() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn demo() -> my_crate::Foo { unimplemented!() }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::TypeReference),
            "expected type reference"
        );
    }

    #[test]
    fn detects_imported_attribute() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            use my_crate::helper;
            #[helper]
            fn demo() {}
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::Attribute && c.function_name == "helper"),
            "expected imported attribute"
        );
    }

    #[test]
    fn detects_nested_macro_path() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn main() { my_crate::nested::foo!(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MacroInvocation && c.function_name == "foo"),
            "expected nested macro invocation"
        );
    }

    #[test]
    fn detects_macro_statement() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn main() { my_crate::foo!{}; }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MacroInvocation),
            "expected macro statement"
        );
    }

    #[test]
    fn detects_method_call_via_imported_name() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            use my_crate::process;
            fn demo() { something.process(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MethodCall && c.function_name == "process"),
            "expected method call attributed to imported name"
        );
    }

    #[test]
    fn detects_method_call_on_crate_path() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn demo() { my_crate::Worker.process(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MethodCall && c.function_name == "process"),
            "expected method call on crate path"
        );
    }

    #[test]
    fn discovers_files_in_examples_tests_benches() {
        let temp = crate::temp::tempdir().unwrap();
        let manifest_path = temp.path().join("Cargo.toml");
        fs::write(
            &manifest_path,
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        let src_dir = temp.path().join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("lib.rs"), "").unwrap();

        for dir in ["examples", "tests", "benches"] {
            let sub = temp.path().join(dir);
            fs::create_dir(&sub).unwrap();
            fs::write(sub.join("demo.rs"), "fn main() {}").unwrap();
        }

        let analyzer = UsageAnalyzer::new(&manifest_path).unwrap();
        let files = analyzer.find_rust_files();
        assert_eq!(files.len(), 4);
    }

    #[test]
    fn guess_item_kind_detects_macro_by_bang() {
        assert!(matches!(guess_item_kind("foo!"), ItemKind::Macro));
    }

    #[test]
    fn detects_nested_import_path() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"use my_crate::nested::Foo;";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(my_crate.imported_items.iter().any(|i| i.name == "Foo"));
    }

    #[test]
    fn detects_macro_through_alias() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            use my_crate as mc;
            fn main() { mc::foo!(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MacroInvocation && c.function_name == "foo"),
            "expected macro through alias"
        );
    }

    #[test]
    fn detects_expr_macro() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            fn main() { let _ = my_crate::foo!(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MacroInvocation),
            "expected expr macro"
        );
    }

    #[test]
    fn detects_method_call_through_alias() {
        let deps = vec![dep_named("my_crate")];
        let mut usage = HashMap::new();
        let code = r"
            use my_crate as mc;
            fn demo() { mc::Type.process(); }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let my_crate = usage.get("my_crate").unwrap();
        assert!(
            my_crate
                .call_sites
                .iter()
                .any(|c| c.kind == UsageKind::MethodCall && c.function_name == "process"),
            "expected method call through alias"
        );
    }

    #[test]
    fn public_function_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub fn demo() -> ext::Foo {
                ext::bar();
                ext::Baz
            }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected pub fn usage to be public API"
        );
    }

    #[test]
    fn private_function_does_not_mark_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            fn demo() {
                ext::bar();
            }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            !ext.used_in_public_api,
            "expected private fn usage to stay private"
        );
    }

    #[test]
    fn public_struct_field_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub struct Demo {
                field: ext::Foo,
            }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected pub struct field to be public API"
        );
    }

    #[test]
    fn public_enum_variant_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub enum Demo {
                A(ext::Foo),
            }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected pub enum variant to be public API"
        );
    }

    #[test]
    fn public_type_alias_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub type MyType = ext::Foo;
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected pub type alias to be public API"
        );
    }

    #[test]
    fn public_use_reexport_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub use ext::Foo;
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected pub use re-export to be public API"
        );
    }

    #[test]
    fn public_impl_method_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub struct S;
            impl S {
                pub fn method(&self) -> ext::Foo {
                    ext::bar()
                }
            }
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected pub impl method to be public API"
        );
    }

    #[test]
    fn trait_impl_for_local_type_marks_used_in_public_api() {
        let deps = vec![dep_named("ext")];
        let mut usage = HashMap::new();
        let code = r"
            pub struct Local;
            impl ext::SomeTrait for Local {}
        ";
        UsageAnalyzer::analyze_file_usage(code, &deps, &mut usage, "test.rs");

        let ext = usage.get("ext").unwrap();
        assert!(
            ext.used_in_public_api,
            "expected trait impl for local type to be public API"
        );
    }
}
