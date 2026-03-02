{
	debug,
	fontconfig,
	pkgs,
	sandbox_run,
	validated_available_renderers,
}:
let
	srcRoot = ./src;
	gitignorePredicate = pkgs.nix-gitignore.gitignoreFilter (builtins.readFile ./.gitignore) srcRoot;
	diagramGeneratorSrc = builtins.path {
		path = srcRoot;
		name = "src-diagram-generator";
		filter = path: type:
			let
				pathStr = toString path;
				rootStr = toString srcRoot;
				relPath = if pathStr == rootStr then "" else pkgs.lib.removePrefix "${rootStr}/" pathStr;
				includePrefixes = [
					"diagram-generator"
					"svg-font-inliner"
				];
				isIncluded =
					relPath == ""
					|| pkgs.lib.any (
						prefix:
						relPath == prefix || pkgs.lib.hasPrefix "${prefix}/" relPath
					) includePrefixes;
			in
			isIncluded && gitignorePredicate pathStr type;
	};
	diagram_generator_rs = pkgs.rustPlatform.buildRustPackage {
		pname = "diagram-generator-rs";
		version = "0.1.0";
		buildType = "release";

		src = diagramGeneratorSrc;
		cargoRoot = "diagram-generator";
		buildAndTestSubdir = "diagram-generator";
		cargoLock = {
			lockFile = ./src/diagram-generator/Cargo.lock;
		};
	};

	bin = pkgs.writeShellScriptBin "diagram-generator" ''
export AVAILABLE_RENDERERS=$(cat <<'EOF'
${builtins.toJSON validated_available_renderers}
EOF
)
export FONTCONFIG_FILE=${fontconfig}
export SVG_FONT_EMBED_DEBUG=${if debug then "1" else "0"}
export PYFTSUBSET_BIN=${pkgs.python3Packages.fonttools}/bin/pyftsubset
export PATH=${pkgs.lib.makeBinPath [pkgs.fontconfig pkgs.python3Packages.fonttools]}:$PATH

case $1 in
    --list-available-renderers)

echo $(cat <<'EOF'
${builtins.toJSON(
builtins.listToAttrs(map
	(engine: {
		name = engine;
		value = map
			(renderer: {
				renderer = builtins.getAttr "renderer" renderer;
				formats = builtins.getAttr "formats" renderer;
			})
			(builtins.getAttr engine validated_available_renderers);
		 }
	)
	(builtins.attrNames validated_available_renderers))
)}
EOF
)

;;
    *)

${sandbox_run}/bin/sandbox-run -- ${diagram_generator_rs}/bin/diagram-generator-rs "$@"

;;
esac

	'';
in {
	inherit bin;
	inherit diagram_generator_rs;
}
