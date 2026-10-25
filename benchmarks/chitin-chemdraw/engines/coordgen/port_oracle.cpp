// Local differential-test driver. This executable is never linked into Chitin.
#include "CoordgenFragmenter.h"
#include "CoordgenMacrocycleBuilder.h"
#include "sketcherMinimizer.h"
#include "sketcherMinimizerAtom.h"
#include "sketcherMinimizerBendInteraction.h"
#include "sketcherMinimizerBond.h"
#include "sketcherMinimizerClashInteraction.h"
#include "sketcherMinimizerEZConstrainInteraction.h"
#include "sketcherMinimizerFragment.h"
#include "sketcherMinimizerMolecule.h"
#include "sketcherMinimizerRing.h"
#include "sketcherMinimizerStretchInteraction.h"
#include <algorithm>
#include <iomanip>
#include <iostream>
#include <map>
#include <string>

static void forces() {
    const float positions[][8] = {
        {0, 0, 40, 10, 60, 0, 10, -25},
        {0, 0, 0, 0, 0, 0, 0, 0},
        {-30, 15, 20, 0, 70, 25, 80, -30},
        {0, 30, 0, 0, 50, 0, 50, 10},
    };
    for (int sample = 0; sample < 4; ++sample) {
        for (int kind = 0; kind < 4; ++kind) {
            sketcherMinimizerAtom a[4];
            for (int i = 0; i < 4; ++i) {
                a[i].coordinates = {
                    positions[sample][i * 2], positions[sample][i * 2 + 1]};
                a[i].force = {0, 0};
            }
            float energy = 0;
            if (kind == 0) {
                sketcherMinimizerStretchInteraction v(&a[0], &a[1]);
                v.k = .1f;
                v.restV = 50;
                v.tolerance = 2;
                v.score(energy);
            }
            if (kind == 1) {
                sketcherMinimizerBendInteraction v(&a[0], &a[1], &a[2]);
                v.score(energy);
            }
            if (kind == 2) {
                sketcherMinimizerClashInteraction v(&a[0], &a[1], &a[2]);
                v.restV = 900;
                v.score(energy);
            }
            if (kind == 3) {
                sketcherMinimizerEZConstrainInteraction v(
                    &a[0], &a[1], &a[2], &a[3], true);
                v.score(energy);
            }
            std::cout << sample << ' ' << kind << ' ' << energy;
            for (auto& atom : a)
                std::cout << ' ' << atom.force.x() << ' ' << atom.force.y();
            std::cout << '\n';
        }
    }
}

static void moves() {
    const float positions[8] = {
        -13.257f, 4.124f, 0, 0, 42.125f, 15.375f, 60.004f, -35.005f};
    sketcherMinimizerAtom atoms[4];
    sketcherMinimizerFragment fragment;
    CoordgenFlipFragmentDOF flip(&fragment);
    CoordgenChangeParentBondLengthFragmentDOF length(&fragment);
    CoordgenRotateFragmentDOF rotate(&fragment);
    for (auto& atom : atoms)
        atom.fragment = &fragment;
    sketcherMinimizerBond bond(&atoms[1], &atoms[2]);
    atoms[1].bonds.push_back(&bond);
    atoms[2].bonds.push_back(&bond);
    CoordgenScaleAtomsDOF scale(&atoms[1]);
    scale.addAtom(&atoms[2]);
    scale.addAtom(&atoms[3]);
    CoordgenInvertBondDOF invert(&atoms[1], &atoms[2]);
    invert.addAtom(&atoms[3]);
    CoordgenFragmentDOF* actions[5] = {
        &flip, &length, &rotate, &scale, &invert};
    const int states[5] = {2, 7, 5, 2, 2};
    for (int kind = 0; kind < 5; ++kind)
        for (int state = 0; state < states[kind]; ++state) {
            fragment._coordinates.clear();
            for (int i = 0; i < 4; ++i) {
                atoms[i].coordinates = {positions[2 * i], positions[2 * i + 1]};
                fragment._coordinates[&atoms[i]] = atoms[i].coordinates;
            }
            auto* action = actions[kind];
            action->setState(state);
            action->apply();
            std::cout << kind << ' ' << state << ' '
                      << action->getCurrentPenalty();
            for (auto& atom : atoms)
                std::cout << ' ' << atom.coordinates.x() << ' '
                          << atom.coordinates.y();
            std::cout << '\n';
        }
}

static void fragments(bool canonical) {
    size_t n, m;
    while (std::cin >> n >> m) {
        sketcherMinimizerMolecule molecule;
        std::map<sketcherMinimizerAtom*, size_t> indices;
        for (size_t i = 0; i < n; ++i) {
            auto* atom = molecule.addNewAtom();
            std::cin >> atom->atomicNumber >> atom->_implicitHs >> atom->charge;
            indices[atom] = i;
        }
        for (size_t i = 0; i < m; ++i) {
            size_t a, b;
            int order;
            std::cin >> a >> b >> order;
            molecule.addNewBond(molecule._atoms.at(a), molecule._atoms.at(b))
                ->bondOrder = order;
        }
        if (canonical)
            sketcherMinimizer::canonicalOrdering(&molecule);
        sketcherMinimizerMolecule::forceUpdateStruct(
            molecule._atoms, molecule._bonds, molecule._rings);
        CoordgenFragmenter::splitIntoFragments(&molecule);
        bool first = true;
        for (auto* fragment : molecule.getFragments()) {
            if (!first)
                std::cout << ';';
            first = false;
            bool first_atom = true;
            for (auto* atom : fragment->getAtoms()) {
                if (!first_atom)
                    std::cout << ',';
                first_atom = false;
                std::cout << indices.at(atom);
            }
            std::cout << '@';
            if (fragment->_bondToParent)
                std::cout << indices.at(fragment->_bondToParent->startAtom)
                          << '>'
                          << indices.at(fragment->_bondToParent->endAtom);
            else
                std::cout << '-';
            std::cout << ':' << fragment->isChain;
        }
        std::cout << '\n';
        for (auto* fragment : molecule.getFragments())
            delete fragment;
        for (auto* bond : molecule._bonds)
            delete bond;
        for (auto* atom : molecule._atoms)
            delete atom;
    }
}

static void coordinates(int stage) {
    const bool supplied_stereo = stage == 11 || stage == 12;
    if (supplied_stereo)
        stage = stage == 11 ? 1 : 0;
    size_t n, m;
    while (std::cin >> n >> m) {
        auto* molecule = new sketcherMinimizerMolecule;
        std::vector<sketcherMinimizerAtom*> source;
        for (size_t i = 0; i < n; ++i) {
            auto* a = molecule->addNewAtom();
            std::cin >> a->atomicNumber >> a->_implicitHs >> a->charge;
            source.push_back(a);
        }
        for (size_t i = 0; i < m; ++i) {
            size_t a, b;
            int order;
            std::cin >> a >> b >> order;
            molecule->addNewBond(molecule->_atoms.at(a), molecule->_atoms.at(b))
                ->bondOrder = order;
        }
        sketcherMinimizer minimizer;
        minimizer.initialize(molecule);
        if (supplied_stereo) {
            size_t count;
            std::cin >> count;
            for (size_t i = 0; i < count; ++i) {
                size_t left, a, b, right;
                int same;
                std::cin >> left >> a >> b >> right >> same;
                auto* bond = source[a]->bondTo(source[b]);
                if (!bond)
                    std::exit(3);
                sketcherMinimizerBondStereoInfo info;
                info.atom1 = source[left];
                info.atom2 = source[right];
                info.stereo = same ? sketcherMinimizerBondStereoInfo::cis
                                   : sketcherMinimizerBondStereoInfo::trans;
                bond->setStereoChemistry(info);
                bond->setAbsoluteStereoFromStereoInfo();
            }
        }
        if (stage == 1 || stage == 3 || stage == 5 || stage == 6 ||
            stage == 7) {
            minimizer.findFragments();
            minimizer.buildFromFragments(true);
            if (stage == 7) {
                bool first = true;
                for (size_t center = 0; center < source.size(); ++center) {
                    for (size_t a = 0; a < source.size(); ++a)
                        for (size_t b = a + 1; b < source.size(); ++b) {
                            if (!source[center]->bondTo(source[a]) ||
                                !source[center]->bondTo(source[b]))
                                continue;
                            auto* winner = sketcherMinimizerAtom::CIPPriority(
                                source[a], source[b], source[center]);
                            if (!first)
                                std::cout << ';';
                            first = false;
                            std::cout << center << '@' << a << ',' << b << '>';
                            if (winner)
                                std::cout << std::distance(source.begin(),
                                    std::find(
                                        source.begin(), source.end(), winner));
                            else
                                std::cout << '-';
                        }
                }
                std::cout << '\n';
                continue;
            }
            if (stage == 6) {
                bool first = true;
                for (auto it = minimizer._fragments.rbegin();
                    it != minimizer._fragments.rend(); ++it) {
                    auto* fragment = *it;
                    for (auto* action : fragment->getDofs()) {
                        if (action->tier() != 1 && action->tier() != 4)
                            continue;
                        if (!first)
                            std::cout << ';';
                        first = false;
                        std::cout << action->tier() << '@';
                        bool first_atom = true;
                        for (size_t i = 0; i < source.size(); ++i) {
                            auto& registered =
                                source[i]->fragment->getDofsOfAtom(source[i]);
                            for (auto* entry : registered)
                                if (entry == action) {
                                    if (!first_atom)
                                        std::cout << ',';
                                    first_atom = false;
                                    std::cout << i;
                                }
                        }
                    }
                }
                std::cout << '\n';
                continue;
            }
            if (stage == 5) {
                bool first = true;
                for (size_t i = 0; i < source.size(); ++i) {
                    auto* atom = source[i];
                    if (atom->neighbors.size() != 4)
                        continue;
                    std::vector<sketcherMinimizerAtomPriority> priorities;
                    for (auto* neighbor : atom->neighbors) {
                        sketcherMinimizerAtomPriority p;
                        p.a = neighbor;
                        priorities.push_back(p);
                    }
                    sketcherMinimizerAtom::orderAtomPriorities(
                        priorities, atom);
                    if (!first)
                        std::cout << ';';
                    first = false;
                    std::cout << i << '@';
                    for (size_t j = 0; j < priorities.size(); ++j) {
                        if (j)
                            std::cout << ',';
                        std::cout << std::distance(source.begin(),
                            std::find(
                                source.begin(), source.end(), priorities[j].a));
                    }
                }
                std::cout << '\n';
                continue;
            }
            if (stage == 3) {
                minimizer.m_minimizer.setupInteractions(true);
                std::cout << minimizer.m_minimizer.scoreInteractions();
                for (auto* atom : source)
                    std::cout << ' ' << atom->force.x() << ' '
                              << atom->force.y();
                std::cout << '\n';
                continue;
            }
        } else if (stage == 2 || stage == 9) {
            minimizer.findFragments();
            if (stage == 9) {
                CoordgenMacrocycleBuilder builder;
                bool first = true;
                for (auto* ring : molecule->_rings) {
                    if (!first)
                        std::cout << ';';
                    first = false;
                    auto* bond = builder.findBondToOpen(ring);
                    if (!bond) {
                        std::cout << '-';
                        continue;
                    }
                    auto a = std::distance(source.begin(),
                        std::find(
                            source.begin(), source.end(), bond->startAtom));
                    auto b = std::distance(source.begin(),
                        std::find(source.begin(), source.end(), bond->endAtom));
                    std::cout << std::min(a, b) << ',' << std::max(a, b);
                }
                std::cout << '\n';
                continue;
            }
            for (auto* fragment : minimizer._fragments) {
                for (auto* ring : fragment->getRings()) {
                    for (auto* atom : ring->_atoms)
                        std::cout << std::distance(source.begin(),
                                         std::find(source.begin(), source.end(),
                                             atom))
                                  << ',';
                    std::cout << ';';
                }
            }
            std::cout << '\n';
            continue;
        } else {
            if (stage == 4)
                minimizer.m_minimizer.skipAvoidClashes = true;
            minimizer.runGenerateCoordinates();
        }
        for (size_t i = 0; i < n; ++i) {
            if (i)
                std::cout << ' ';
            std::cout << source[i]->coordinates.x() / 50.f << ' '
                      << source[i]->coordinates.y() / 50.f;
        }
        std::cout << '\n';
    }
}

int main(int argc, char** argv) {
    std::cout << std::setprecision(9);
    if (argc == 2 && std::string(argv[1]) == "--templates") {
        sketcherMinimizer::loadTemplates();
        size_t index = 0;
        for (auto* molecule : sketcherMinimizer::m_templates.getTemplates()) {
            std::cout << index++;
            for (auto* atom : molecule->_atoms)
                std::cout << ' ' << atom->coordinates.x() << ' '
                          << atom->coordinates.y();
            std::cout << '\n';
        }
    } else if (argc == 2 && std::string(argv[1]) == "--forces")
        forces();
    else if (argc == 2 && std::string(argv[1]) == "--moves")
        moves();
    else if (argc == 2 && std::string(argv[1]) == "--action-scopes")
        coordinates(6);
    else if (argc == 2 && std::string(argv[1]) == "--rankings")
        coordinates(7);
    else if (argc == 2 && std::string(argv[1]) == "--opening-edges")
        coordinates(9);
    else if (argc == 2 && std::string(argv[1]) == "--assembled-stereo")
        coordinates(11);
    else if (argc == 2 && std::string(argv[1]) == "--coordinates-stereo")
        coordinates(12);
    else if (argc == 2 && std::string(argv[1]) == "--priorities")
        coordinates(5);
    else if (argc == 2 && std::string(argv[1]) == "--fragments")
        fragments(false);
    else if (argc == 2 && std::string(argv[1]) == "--canonical-fragments")
        fragments(true);
    else if (argc == 2 && std::string(argv[1]) == "--coordinates")
        coordinates(0);
    else if (argc == 2 && std::string(argv[1]) == "--assembled")
        coordinates(1);
    else if (argc == 2 && std::string(argv[1]) == "--rings")
        coordinates(2);
    else if (argc == 2 && std::string(argv[1]) == "--assembly-forces")
        coordinates(3);
    else if (argc == 2 && std::string(argv[1]) == "--oriented-assembly")
        coordinates(4);
    else
        return 2;
}
