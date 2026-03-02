{
	plantuml_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).plantuml,
	recharts_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).recharts,
	swirly_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).swirly,
	debug ? false,
	fontconfig,
	pkgs,
}:
let
	sandbox_run = import ./sandbox-run.nix { inherit debug pkgs; };
	svg_font_inliner = import ./svg-font-inliner.nix {
		inherit debug fontconfig pkgs;
	};
	svg_to_png = import ./svg-to-png.nix {
		inherit debug pkgs;
	};
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

	available_renderers = {
		"plantuml" = (map
			({version, formats}: {
				bin = (import (./plantuml + "/${version}.nix")).bin {inherit version fontconfig;};
				version = version;
				formats = formats;
				renderer = ''plantuml-${version}'';
			})
			plantuml_versions
		);
		"recharts" = (map
			({version, formats}: {
				bin = (import (./recharts + "/${version}.nix")).bin {version = version;};
				version = version;
				formats = formats;
				renderer = ''recharts-${version}'';
			})
			recharts_versions
		);
		"swirly" = (map
			({version, formats}: {
				bin = (import (./swirly + "/${version}.nix")).bin {version = version;};
				version = version;
				formats = formats;
				renderer = ''swirly-${version}'';
			})
			swirly_versions
		);
	};

	validated_available_renderers = if pkgs.lib.allUnique
	(builtins.map
		(config: config.renderer)
		(builtins.concatLists (builtins.attrValues available_renderers))
	)
	then available_renderers
	else throw "renderer strings not unique";

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
	inherit sandbox_run;
	inherit svg_font_inliner;
	inherit svg_to_png;
}
