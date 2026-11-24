.PHONY: startdocker
startdocker:
	@if docker info >/dev/null 2>&1; then \
		echo "Docker is already running."; \
	else \
		echo "Starting Docker..."; \
		if [ "$$(uname)" = "Darwin" ]; then \
			open -a Docker; \
		else \
			sudo systemctl start docker; \
		fi; \
		echo "Waiting for Docker to fully initialize..."; \
		until docker info >/dev/null 2>&1; do sleep 1; done; \
		echo "Docker is ready!"; \
	fi

.PHONY: checkvenv
checkvenv:
# raises error if environment is not active
ifeq ("$(VIRTUAL_ENV)","")
	@echo "Venv is not activated!"
	@echo "Activate venv first."
	@echo
	exit 1
endif

.PHONY: pipupgrade
pipupgrade: checkvenv
	# pip-tools is not ready for pip 26
	pip install pip==25.2
	# pip install --upgrade pip

.PHONY: pyupgrade
pyupgrade: checkvenv pipupgrade
# checks if pip-tools is installed
ifeq ("$(wildcard .venv/bin/pip-compile)","")
	@echo "Installing Pip-tools..."
	# @pip install --no-cache-dir pip-tools
	# pip-tools is not ready for pip 26
	@pip install --no-cache-dir pip-tools==7.5.2
endif

ifeq ("$(wildcard .venv/bin/pip-sync)","")
	@echo "Installing Pip-tools..."
	# @pip install --no-cache-dir pip-tools
	# pip-tools is not ready for pip 26
	@pip install --no-cache-dir pip-tools==7.5.2
endif

.PHONY: install-cython
install-cython: checkvenv
	pip install cython

.PHONY: build
build: checkvenv startdocker down clean-mypyc-artifacts install-cython local-install-deps-dev local-install-deps-prod
	export IMAGE_TAG=local && \
	docker build --progress=plain -t wycliffeassociates/doc-usfmparser:$${IMAGE_TAG} ./dotnet/USFMParserApi && \
	docker build --progress=plain -t wycliffeassociates/doc:$${IMAGE_TAG} . && \
	docker build --progress=plain -t wycliffeassociates/doc-ui:$${IMAGE_TAG} ./frontend && \
        docker build --progress=plain -t wycliffeassociates/doc-ui-tests:$${IMAGE_TAG} -f ./frontend/testsDockerfile ./frontend

.PHONY: build-no-pip-update
build-no-pip-update: checkvenv startdocker down clean-mypyc-artifacts
	export IMAGE_TAG=local && \
	docker build --progress=plain -t wycliffeassociates/doc-usfmparser:$${IMAGE_TAG} ./dotnet/USFMParserApi && \
	docker build --progress=plain -t wycliffeassociates/doc:$${IMAGE_TAG} . && \
	docker build --progress=plain -t wycliffeassociates/doc-ui:$${IMAGE_TAG} ./frontend && \
        docker build --progress=plain -t wycliffeassociates/doc-ui-tests:$${IMAGE_TAG} -f ./frontend/testsDockerfile ./frontend

.PHONY: build-no-pip-update-run-tests
build-no-pip-update-run-tests: checkvenv startdocker down clean-mypyc-artifacts
	export IMAGE_TAG=local && \
	docker build --progress=plain -t wycliffeassociates/doc-usfmparser:$${IMAGE_TAG} ./dotnet/USFMParserApi && \
	docker build --build-arg RUN_TESTS=true --progress=plain -t wycliffeassociates/doc:$${IMAGE_TAG} . && \
	docker build --progress=plain -t wycliffeassociates/doc-ui:$${IMAGE_TAG} ./frontend && \
        docker build --progress=plain -t wycliffeassociates/doc-ui-tests:$${IMAGE_TAG} -f ./frontend/testsDockerfile ./frontend

.PHONY: build-no-cache
build-no-cache: checkvenv startdocker down clean-mypyc-artifacts local-install-deps-prod
	export IMAGE_TAG=local && \
	docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc-usfmparser:$${IMAGE_TAG} ./dotnet/USFMParserApi && \
	docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc:$${IMAGE_TAG} . && \
	docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc-ui:$${IMAGE_TAG} ./frontend && \
        docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc-ui-tests:$${IMAGE_TAG} -f ./frontend/testsDockerfile ./frontend

.PHONY: build-no-cache-no-pip-update
build-no-cache-no-pip-update: checkvenv startdocker down clean-mypyc-artifacts
	export IMAGE_TAG=local && \
	docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc-usfmparser:$${IMAGE_TAG} ./dotnet/USFMParserApi && \
	docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc:$${IMAGE_TAG} . && \
	docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc-ui:$${IMAGE_TAG} ./frontend && \
        docker build --progress=plain --no-cache --pull -t wycliffeassociates/doc-ui-tests:$${IMAGE_TAG} -f ./frontend/testsDockerfile ./frontend

todays_date := $(shell date +%FT%T%Z)
doc_version := $(shell git rev-parse --short HEAD)
.PHONY: up
up: checkvenv startdocker
	export IMAGE_TAG=local && \
	PUBLIC_DOC_VERSION=$(doc_version) PUBLIC_DOC_BUILD_TIMESTAMP=$(todays_date) PUBLIC_LOGROCKET_ID=ct7zyg/interleaved-resource-generator DATA_API_URL=https://api.bibleineverylanguage.org/v1/graphql PUBLIC_BACKEND_API_URL=http://localhost:5005 PUBLIC_FILE_SERVER_URL=http://localhost:8089 docker compose up --remove-orphans

.PHONY: up-as-daemon
up-as-daemon: checkvenv startdocker
	export IMAGE_TAG=local && \
	LOGROCKET_ID=ct7zyg/interleaved-resource-generator BACKEND_API_URL=http://localhost:5005 FILE_SERVER_URL=http://localhost:8089 docker compose up -d


# This is the entrypoint for a non-technical user who just
# wants to type one command and have it work.
.PHONY: build-and-run
build-and-run: build up


.PHONY: down
down: startdocker
	LOGROCKET_ID=ct7zyg/interleaved-resource-generator BACKEND_API_URL=http://localhost:5005 FILE_SERVER_URL=http://localhost:8089 docker compose down --remove-orphans

.PHONY: stop-and-remove
stop-and-remove: startdocker
	docker ps -q | xargs docker stop
	docker ps -a -q -f status=exited | xargs docker rm

.PHONY: list-pytest-markers
list-pytest-markers:
	pytest --strict-markers

.PHONY: clean-local-docker-output-dir
clean-local-docker-output-dir:
	find docker_document_output/ -type f -name "*.pdf" -exec rm -- {} +
	find docker_document_output/ -type f -name "*.docx" -exec rm -- {} +
	find docker_document_output/ -type f -name "*.html" -exec rm -- {} +

.PHONY: test
test: startdocker clean-local-docker-output-dir
	docker compose -f docker-compose.yml -f docker-compose.api-test.yml -f docker-compose.override.yml up  test-runner

.PHONY: unit-tests
unit-tests: startdocker
	docker compose -f docker-compose.yml -f docker-compose.api-test.yml -f docker-compose.override.yml up  test-runner

.PHONY: e2e-tests
e2e-tests: startdocker clean-local-docker-output-dir
	docker compose -f docker-compose.yml -f docker-compose.api-test.yml -f docker-compose.override.yml up  test-runner


.PHONY: e2e-docx-tests
e2e-docx-tests: startdocker clean-local-docker-output-dir
	docker compose -f docker-compose.yml -f docker-compose.api-docx-test.yml -f docker-compose.override.yml up  test-runner

.PHONY: test-durations
test-durations: startdocker
	-docker rm -f doc-test-durations 2>/dev/null
	-docker compose -f docker-compose.yml -f docker-compose.api-test.yml -f docker-compose.override.yml \
	  run --name doc-test-durations test-runner \
	  -n auto -m "not all and not randomized and not english_randomized" \
	  --store-durations --durations-path /app/tests/.test_durations \
	  /app/tests/unit /app/tests/e2e
	docker cp doc-test-durations:/app/tests/.test_durations tests/.test_durations
	docker rm doc-test-durations

.PHONY: frontend-tests
frontend-tests: startdocker
	# NOTE If we are experiencing some issues with the docker
	# compose running of frontend tests, we can still use the
	# non-Dockerized approach successfully. Doing so requires that
	# 'make up' be called first though.
	docker compose -f docker-compose.yml -f docker-compose.frontend-test.yml up --exit-code-from frontend-test-runner

.PHONY: test-randomized
test-randomized: startdocker
	LOGROCKET_ID=ct7zyg/interleaved-resource-generator BACKEND_API_URL=http://localhost:5005 docker compose run --rm --no-deps --entrypoint=pytest api -v -m randomized -n auto /app/tests/unit /app/tests/e2e

# NOTE This is only needed if mypyc is not used and you wish to run
# just mypy instead as part of Docker build process.
# .PHONY: get-usfm-tools-source-locally
# get-usfm-tools-source-locally:
# 	# In Docker we build usfm_tools package into a .so, but outside
# 	# Docker we need the source to let mypy check it locally, i.e., when
# 	# checking outside Docker build.
# 	cd /tmp && \
# 	git clone -b develop --depth 1 https://github.com/linearcombination/USFM-Tools  && \
# 	cd ./USFM-Tools && \
# 	cp -r ./usfm_tools ${VIRTUAL_ENV}/lib/python3.11/site-packages/

# .PHONY: build-usfm-tools
# build-usfm-tools:
# 	cd /tmp && \
# 	git clone -b develop --depth 1 https://github.com/linearcombination/USFM-Tools
# 	cd /tmp/USFM-Tools && python setup.py build install
# 	cp -r /tmp/USFM-Tools/usfm_tools ~/src/WA/github.com/linearcombination/DOC/.venv/lib/python3.11/site-packages/
# 	rm -rf /tmp/USFM-Tools

# You may need to run 'make build-usfm-tools' first if it
# complains about usfm_tools not being available or typed.
.PHONY: mypy
mypy: checkvenv
	mypy --strict --install-types --non-interactive backend/doc/**/*.py
	mypy --strict --install-types --non-interactive backend/stet/**/*.py
	mypy --strict --install-types --non-interactive backend/passages/**/*.py
	mypy --strict --install-types --non-interactive tests/**/*.py

.PHONY: mypyc
mypyc:
	cd backend && mypyc --strict --install-types --non-interactive --verbose doc/domain/assembly_strategies.py doc/domain/parsing.py doc/domain/resource_lookup.py # doc/domain/document_generator.py


.PHONY: clean-mypyc-artifacts
clean-mypyc-artifacts:
	find . ! -path .venv -type f -name "*.so" -exec rm -- {} +
	find . ! -path .venv -type f -name "*.c" -exec rm -- {} +

.PHONY: prune-docker-images-volumes
prune-docker-images-volumes: checkvenv startdocker
	docker system prune --volumes

# https://radon.readthedocs.io/en/latest/commandline.html
.PHONY: radon-cyclomatic-complexity
radon-cyclomatic-complexity: checkvenv
	radon cc backend/doc/**/*.py
	radon cc backend/stet/**/*.py
	radon cc backend/passages/**/*.py

.PHONY: radon-raw-stats
radon-raw-stats: checkvenv
	radon raw backend/doc/**/*.py
	radon raw backend/stet/**/*.py
	radon raw backend/passages/**/*.py

.PHONY: radon-maintainability-index
radon-maintainability-index: checkvenv
	radon mi backend/doc/**/*.py
	radon mi backend/stet/**/*.py
	radon mi backend/passages/**/*.py

.PHONY: radon-halstead-complexity
radon-halstead-complexity: checkvenv
	radon hal backend/doc/**/*.py
	radon hal backend/stet/**/*.py
	radon hal backend/passages/**/*.py

.PHONY: vulture-dead-code
vulture-dead-code: checkvenv
	vulture backend/doc/ --min-confidence 100
	vulture backend/stet/ --min-confidence 100
	vulture backend/passages/ --min-confidence 100
	vulture tests/ --min-confidence 100

.PHONY: generate-class-diagrams
generate-class-diagrams:
	pyreverse backend
	dot -Tpng classes.dot -o docs/classes.png

.PHONY: all
all: down build up test

.PHONY: all-plus-linting
all-plus-linting: mypy down build up test

# Run a local Uvicorn server outside Docker
.PHONY: local-server
local-server: checkvenv
	PUBLIC_DOC_VERSION=$(doc_version) PUBLIC_DOC_BUILD_TIMESTAMP=$(todays_date) PUBLIC_LOGROCKET_ID=ct7zyg/interleaved-resource-generator DATA_API_URL=https://api.bibleineverylanguage.org/v1/graphql PUBLIC_BACKEND_API_URL=http://localhost:5005 PUBLIC_FILE_SERVER_URL=http://localhost:8089 uvicorn doc.entrypoints.app:app --reload --host "0.0.0.0" --port "5005" --app-dir "./backend/"

# Run a local Gunicorn server outside Docker
.PHONY: local-gunicorn-server
local-gunicorn-server: checkvenv
	exec gunicorn --name DOC --worker-class uvicorn.workers.UvicornWorker --conf ./backend/gunicorn.conf.py --pythonpath ./backend  doc.entrypoints.app:app

.PHONY: local-update-deps-base
local-update-deps-base: pyupgrade
	pip-compile --resolver=backtracking -v ./backend/requirements.in
	# pip-compile --upgrade ./backend/requirements.in

.PHONY: local-update-deps-prod
local-update-deps-prod: local-update-deps-base
	pip-compile --resolver=backtracking -v ./backend/requirements-prod.in
	# pip-compile --upgrade ./backend/requirements-prod.in

.PHONY: local-update-deps-dev
local-update-deps-dev: local-update-deps-base
	pip-compile --resolver=backtracking -v ./backend/requirements-dev.in
	# pip-compile --upgrade ./backend/requirements-dev.in

.PHONY: local-install-deps-base
local-install-deps-base: install-cython local-update-deps-base
	pip install --no-cache-dir -r ./backend/requirements.txt

.PHONY: local-install-deps-dev
local-install-deps-dev: local-update-deps-dev local-install-deps-base
	pip install --no-cache-dir -r ./backend/requirements-dev.txt

.PHONY: local-install-deps-prod
local-install-deps-prod: local-update-deps-prod local-install-deps-base
	pip install --no-cache-dir -r ./backend/requirements-prod.txt

.PHONY: local-prepare-for-tests
local-prepare-for-tests: mypy local-clean-working-output-dir

.PHONY: local-prepare-for-tests-without-cleaning
local-prepare-for-tests-without-cleaning: mypy

.PHONY: local-clean-working-output-dir
local-clean-working-output-dir:
	find assets_download/ -type f -name "*.html" -exec rm -- {} +
	find assets_download/ -type f -name "*.pdf" -exec rm -- {} +

.PHONY: local-install-app
local-install-app:
	pip install -e .

.PHONY: local-unit-tests
local-unit-tests:  local-prepare-for-tests
	TO_EMAIL="foo@example.com" pytest -n auto tests/unit/ -vv

.PHONY: local-e2e-tests
local-e2e-tests:  local-prepare-for-tests
	TO_EMAIL="foo@example.com" pytest -n auto tests/e2e/ -vv

.PHONY: local-repeat-randomized-tests
local-repeat-randomized-tests: local-prepare-for-tests
	TO_EMAIL="foo@example.com" pytest -n auto --count 10 tests/e2e/ -k test_api_randomized_combinatoric.py

.PHONY: local-run-celery
local-run-celery:
	celery -A doc.domain.worker.app worker --loglevel=DEBUG -E


.PHONY: local-run-flower
local-run-flower:
	celery --broker=redis:// --result-backend=redis:// flower

# Run after running local-e2e-tests or any tests which has yielded HTML
# and PDFs that need to be checked for linking correctness.
.PHONY: local-check-anchor-links
local-check-anchor-links: checkvenv
	python tests/e2e/test_anchor_linking.py
