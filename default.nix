{
	plantuml_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).plantuml,
	recharts_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).recharts,
	swirly_versions ? (builtins.fromJSON (builtins.readFile ./supported-versions.json)).swirly,
	fontconfig,
	pkgs,
}:
let
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
	--env TMP \
	--env TEMP \
	--env TMPDIR \
${pkgs.nodejs_latest}/bin/node ${./src/index.ts} "$@"

;;
esac

	'';
in
	bin
