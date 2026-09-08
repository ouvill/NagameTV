#ifndef VIEWER_COMMENT_MODEL_TEST_H
#define VIEWER_COMMENT_MODEL_TEST_H
#include <QtTest/QAbstractItemModelTester>
// Only included by the qml_tests feature. TestCase.failOnWarning catches failures.
template<typename T>
inline void attachCommentModelTester(T &model) {
    new QAbstractItemModelTester(&model, QAbstractItemModelTester::FailureReportingMode::Warning, &model);
}

#endif
