#!/bin/bash

# --- Configuration ---
# How many subfolders to put in each archive
CHUNK_SIZE=20
# The prefix for your archive names
ARCHIVE_PREFIX="LKML5Ws-linux-mailing-lists-columnar-dataset_anonymized"
SOURCE_DIR="../output/anonymizer/__main_dataset/"
LINEAGE_SOURCE_DIR="../output/parser/lineage/"
# ---------------------

# 1. Check if the source directory exists
if [ ! -d "$SOURCE_DIR" ]; then
	echo "Error: Source directory does not exist: $SOURCE_DIR"
	exit 1
fi

# 2. Get all subfolders into an array.
# We use 'find' to get only the relative names of the directories.
# 'mapfile' reads the output of 'find' directly into the 'folders' array.
# This correctly handles names with spaces.
mapfile -t folders < <(find "$SOURCE_DIR" -mindepth 1 -maxdepth 1 -type d -printf "%f\n")

# Check if any folders were found
if [ ${#folders[@]} -eq 0 ]; then
	echo "Error: No subfolders found in $SOURCE_DIR."
	exit 1
fi

total_folders=${#folders[@]}
echo "Found $total_folders total subfolders."
echo "Archives will be created in: $(pwd)"

# 3. Loop through the array in chunks
for ((i = 0; i < $total_folders; i += $CHUNK_SIZE)); do
	# Get a "slice" of the array (e.g., folders 0-9, then 10-19, etc.)
	chunk=("${folders[@]:$i:$CHUNK_SIZE}")

	# Define the archive name (e.g., archive_part_1.tar.gz)
	part_num=$(((i / CHUNK_SIZE) + 1))
	archive_name="${ARCHIVE_PREFIX}_${part_num}.tar.gz"

	echo "---"
	echo "Creating $archive_name..."
	echo "Adding ${#chunk[@]} folders:"
	# 'printf' is safer than 'echo' for printing the list
	printf "  %s\n" "${chunk[@]}"

	# 4. Create the compressed tarball for this chunk
	#
	# '-C "$SOURCE_DIR"' : Tells tar to change to that directory first.
	#                      This keeps the paths in the archive relative (e.g., "subfolder1/...")
	#                      instead of absolute (e.g., "/path/to/TopLevelFolder/subfolder1/...")
	# '-cvf -'           : Create, Verbose, File, and send to stdout (-)
	# "${chunk[@]}"      : The list of relative subfolder names to add
	# '| gzip -9'        : Pipe the tar output to gzip with max compression (-9)
	# '> "$archive_name"': Redirect the compressed output to the archive file

	tar -C "$SOURCE_DIR" -cvf - "${chunk[@]}" | gzip -9 >"$archive_name"

	echo "Successfully created $archive_name."
done

lineage_archive_name="data-lineage_${ARCHIVE_PREFIX}.tar.gz"
tar -C "$(dirname "$LINEAGE_SOURCE_DIR")" -cvf - lineage | gzip -9 >"$lineage_archive_name"

echo "---"
echo "All done!"
