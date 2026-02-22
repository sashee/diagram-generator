{
	plantuml_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).plantuml,
	recharts_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).recharts,
	swirly_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).swirly,
	fontconfig,
	pkgs,
}:
let
	svg_font_extractor = import ./svg-font-extractor.nix {};

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
export SVG_FONT_EXTRACTOR_BIN=${svg_font_extractor}/bin/svg-font-extractor
export FONTCONFIG_FILE=${fontconfig}

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

# no network access
${pkgs.landrun}/bin/landrun \
	--unrestricted-filesystem \
	--env AVAILABLE_RENDERERS \
	--env SVG_FONT_EXTRACTOR_BIN \
	--env FONTCONFIG_FILE \
	--env TMP \
	--env TEMP \
	--env TMPDIR \
${pkgs.nodejs_latest}/bin/node ${./src/index.ts} "$@"

;;
esac

	'';
in {
	inherit bin;
}
