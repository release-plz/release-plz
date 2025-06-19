# generate-schema

The `release-plz generate-schema` command will generate a JSON schema for the configuration file.
The file will be generated as `.schema/latest.json`.

This command is mostly meant for development purposes and will be generated when new configurations
are added. It will be referenced on [SchemaStore](https://www.schemastore.org/) to allow
supported IDEs to autocomplete and validate the configuration file.
