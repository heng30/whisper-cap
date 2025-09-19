#!/bin/sh

if [ $# -ne 1 ] || [ "$1" == "-h" ] || [ "$1" == "--help" ] || [ "$1" == "-help" ]; then
    echo "Usage: $0 <new-project-name>"
    exit
fi

LOC=$(readlink -f "$0")
DIR=$(dirname "$LOC")

project_name=$1
old_project_name="whisper-cap"

mv $DIR/package/usr/share/applications/${old_project_name}.desktop $DIR/package/usr/share/applications/${project_name}.desktop 2>/dev/null

find "$DIR" -type f ! -path "$DIR/$(basename "$0")" -exec sed -i "s/${old_project_name}/${project_name}/g" {} +

exit $?
