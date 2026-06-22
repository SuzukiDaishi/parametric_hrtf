cmake_minimum_required(VERSION 3.28)

set(CMAKE_CXX_STANDARD 17)

project(wclap-cpp-browser VERSION 1.0.0)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_CURRENT_LIST_DIR}/../")
set(CMAKE_EXECUTABLE_SUFFIX .wasm)

add_executable(host
	${CMAKE_CURRENT_LIST_DIR}/source/host.cpp
	${CMAKE_CURRENT_LIST_DIR}/source/cbor-bytes.cpp
)
target_compile_options(host PUBLIC "-fno-exceptions")
target_link_options(host PUBLIC "-mexec-model=reactor" "-Wl,--max-memory=2147483648,--export-table,--growable-table")

# Add wclap-js
add_subdirectory(../wclap-js "wclap-js-cpp-build")
target_link_libraries(host PUBLIC wclap-js)

# CBOR for passing structured results
add_subdirectory(modules/cbor-walker)
target_link_libraries(host PUBLIC cbor-walker)
