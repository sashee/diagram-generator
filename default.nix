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
			(version: {
				bin = (import (./plantuml + "/${version}.nix")).bin {inherit version fontconfig;};
				version = version;
			})
			(pkgs.lib.lists.naturalSort plantuml_versions));
		"recharts" = (map
			(version: {
				bin = (import (./recharts + "/${version}.nix")).bin {version = version;};
				version = version;
			})
			(pkgs.lib.lists.naturalSort recharts_versions));
		"swirly" = (map
			(version: {
				bin = (import (./swirly + "/${version}.nix")).bin {version = version;};
				version = version;
			})
			(pkgs.lib.lists.naturalSort swirly_versions));
	};

	bin = pkgs.writeShellScriptBin "diagram-generator" ''
export AVAILABLE_RENDERERS=$(cat <<'EOF'
${builtins.toJSON available_renderers}
EOF
)

case $1 in
    --list-available-renderers)

echo $(cat <<'EOF'
${builtins.toJSON
(map
	(engine: (
		map
			(renderer: ''${engine}-${renderer.version}'')
			(builtins.getAttr engine available_renderers)
	))
	(builtins.attrNames available_renderers))
}
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
