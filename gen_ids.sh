for m in modules/*/ ; do
    NAME=$(echo "$m" | sed s/[^/]*\\/// | sed s/\\///)
    b3sum --raw target/wasm32-unknown-unknown/release/$NAME.wasm > modules/$NAME/id    
done
